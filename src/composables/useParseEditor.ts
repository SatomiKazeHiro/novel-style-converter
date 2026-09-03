import { onBeforeUnmount, shallowRef, type ComputedRef, type Ref } from 'vue';
import type { EditorView as EditorViewType, DecorationSet as DecorationSetType } from '@codemirror/view';
import type { EditorState as EditorStateType } from '@codemirror/state';
import { isVisuallyEmptyLine } from '../utils/splitChapters';

import { StateField, StateEffect, RangeSetBuilder } from '@codemirror/state';
import { Decoration, GutterMarker } from '@codemirror/view';

export interface UseParseEditorOptions {
  host: Ref<HTMLElement | null>;
  onBoundaryToggle?: (line1based: number) => void;
  boundarySet?: ComputedRef<Set<string>>;
}

export interface UseParseEditorApi {
  view: Readonly<Ref<EditorViewType | null>>;
  setMarkers: (lines1based: ReadonlySet<number>) => void;
  scrollToLine: (line0based: number) => void;
  runSearch: (query: string) => void;
  nextHit: () => void;
  prevHit: () => void;
  hitCount: Readonly<Ref<number>>;
  currentHitIndex: Readonly<Ref<number>>;
  replaceDoc: (text: string) => void;
  mount: (doc: string) => Promise<void>;
  destroy: () => void;
}

export function useParseEditor(opts: UseParseEditorOptions): UseParseEditorApi {
  const view = shallowRef<EditorViewType | null>(null);
  const hitCount = shallowRef(0);
  const currentHitIndex = shallowRef(0);

  function destroy() {
    view.value?.destroy();
    view.value = null;
  }
  // Module references to CM6 runtime modules, populated in mount().
  let cmSearchMod: typeof import('@codemirror/search') | null = null;
  let cmViewEditor: typeof import('@codemirror/view').EditorView | null = null;


  // Boundary StateField is built once per mount.
  const boundaryEffect = StateEffect.define<ReadonlySet<number>>();
  const boundaryField = StateField.define<ReadonlySet<number>>({
    create: () => new Set<number>(),
    update: (value, tr) => {
      for (const e of tr.effects) {
        if (e.is(boundaryEffect)) return e.value;
      }
      return value;
    },
  });

  // Decoration builder: rebuild RangeSet of "marked line" decorations on boundary change.
  const boundaryLineDeco = (state: EditorStateType): DecorationSetType => {
    const set = state.field(boundaryField, false) ?? new Set<number>();
    const builder = new RangeSetBuilder<Decoration>();
    // RangeSetBuilder requires ranges added in (non-overlapping) order by
    // rom position. Set iteration is insertion order, not numeric, and
    // the chapters store sorts markers lexicographically (so "12" < "9" in
    // Set order) — explicit numeric sort before add() avoids CM6's internal
    // tree code throwing "a[i].compare is not a function".
    const sortedLines = Array.from(set).sort((a, b) => a - b);
    for (const line1based of sortedLines) {
      try {
        const line = state.doc.line(line1based);
        builder.add(line.from, line.from, Decoration.line({ attributes: { class: 'cm-marker-line' } }));
      } catch {
        // line out of range (e.g. doc shrunk); skip
      }
    }
    return builder.finish();
  };

  // Substring-based hit cursor (mirrors the previous useChapterSearch semantics:
  // plain string contains, literal mode). Drives hitCount / currentHitIndex.
  // CM's search() extension handles its own highlighting in parallel via
  // setSearchQuery below; the substring cursor only feeds the toolbar counter.
  let currentQuery = '';
  let searchLines: string[] = [];
  let searchHits: number[] = [];
  let searchCursor = 0;

  function rebuildSearchIndex(doc: string): void {
    searchLines = doc.split('\n');
    searchHits = [];
    if (currentQuery) {
      for (let i = 0; i < searchLines.length; i++) {
        if (searchLines[i].includes(currentQuery)) searchHits.push(i);
      }
    }
    searchCursor = 0;
    hitCount.value = searchHits.length;
    currentHitIndex.value = searchHits.length === 0 ? 0 : 1;
  }

  /// 每行一份独立的章按钮 DOM — CM6 不允许同一份节点跨多个 gutter slot 复用。
  /// marker 状态不在 DOM 上区分,而是改由 RangeSet<Decoration> 在已盖行画背景(`.cm-marker-line`),
  /// 与原版 MarkerButton.vue 行为等价:每行可点,已点的行加底色。
    function makeStamp(lineFrom: number): HTMLElement {
      const el = document.createElement('button');
      el.type = 'button';
      el.className = 'cm-marker-stamp';
      el.title = '在此拆分 / 取消标记';
      el.textContent = '章';
      // 把当前 boundary 状态反映到按钮 class 上 ——
      // 否则用户点完看不到任何视觉变化,会以为 click 没生效。
      // opts.boundarySet 由外部传入(随 store.boundaries 反应式更新)。
      function syncBoundaryClass() {
        const v = view.value;
        if (!v || !opts.boundarySet) return;
        const line1based = v.state.doc.lineAt(lineFrom).number;
        const key = String(line1based - 1); // store 0-based
        el.classList.toggle('cm-marker-stamp--marked', opts.boundarySet.value.has(key));
      }
      syncBoundaryClass();
      el.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const v = view.value;
        if (!v || !opts.onBoundaryToggle) return;
        const line1based = v.state.doc.lineAt(lineFrom).number;
        opts.onBoundaryToggle(line1based);
        // 立刻反映新状态;不依赖 store 反应式回环,给用户即时反馈。
        setTimeout(syncBoundaryClass, 0);
      });
      return el;
    }
  /// CM6 gutter 要求 lineMarker 返回 GutterMarker 实例(不是裸 DOM);
  /// setMarkers 会调用 marker.compare 做差分,无 compare 就崩。
  /// 不同行的 from 不同 → eq() 返回 false → CM6 不会跨行复用 DOM,每行各一份按钮。
  class BoundaryStamp extends GutterMarker {
    constructor(public readonly lineFrom: number) { super(); }
    toDOM() {
      return makeStamp(this.lineFrom);
    }
    eq(other: GutterMarker) {
      return other instanceof BoundaryStamp && other.lineFrom === this.lineFrom;
    }
  }
  const boundaryGutter = {
    class: 'cm-marker-gutter',
    lineMarker(view: EditorViewType, lineBlock: { from: number }) {
      const text = view.state.doc.lineAt(lineBlock.from).text;
      if (isVisuallyEmptyLine(text)) return null;
      return new BoundaryStamp(lineBlock.from);
    },
  } as const;

  async function mount(doc: string): Promise<void> {
    const host = opts.host.value;
    if (!host) return;
    view.value?.destroy();
    view.value = null;

    const [
      { EditorState },
      { EditorView, drawSelection, lineNumbers, gutter },
      cmCommands,
      _cmSearch,
    ] = await Promise.all([
      import('@codemirror/state'),
      import('@codemirror/view'),
      import('@codemirror/commands'),
      import('@codemirror/search'),
    ]) as [
      typeof import('@codemirror/state'),
      typeof import('@codemirror/view'),
      typeof import('@codemirror/commands'),
      typeof import('@codemirror/search'),
    ];
    cmViewEditor = EditorView;
    cmSearchMod = _cmSearch;

    const themeExt = EditorView.theme({
      '&': {
        height: '100%',
        fontSize: '13px',
        fontFamily: 'var(--font-mono), ui-monospace, monospace',
        color: 'var(--text-primary)',
        backgroundColor: 'var(--color-sheet)',
      },
      '&.cm-focused': { outline: 'none' },
      '.cm-content': {
        padding: '8px 12px',
        caretColor: 'var(--color-cinnabar)',
      },
      '.cm-line': { lineHeight: '24px' },
      '.cm-scroller': { fontFamily: 'inherit' },
      '.cm-gutters': {
        backgroundColor: 'transparent',
        borderRight: '1px solid var(--border-color)',
        color: 'var(--text-muted)',
      },
      // Marker gutter column. alignItems centers the button horizontally;
      // do NOT set justifyContent -- CM6 forces minHeight:100% so a centered
      // flex pack pushes the buttons to the vertical middle of the document.
      '.cm-gutter.cm-marker-gutter': {
        display: 'flex',
        alignItems: 'center',
        flexShrink: 0,
        width: '28px',
        cursor: 'default',
        boxSizing: 'border-box',
      },
      // Marked-line background (driven by RangeSet<Decoration>).
      '.cm-marker-line': {
        backgroundColor: 'var(--color-cinnabar-light)',
      },
      // Per-line stamp button (visually mirrors the retired MarkerButton.vue).
      '.cm-marker-stamp': {
        boxSizing: 'border-box',
        width: '22px',
        height: '20px',
        padding: '0',
        background: 'var(--color-cinnabar)',
        border: 'none',
        color: 'var(--color-sheet)',
        fontFamily: "'Songti SC', 'STSong', 'SimSun', 'Source Han Serif SC', serif",
        fontSize: '13px',
        fontWeight: 700,
        lineHeight: '18px',
        cursor: 'pointer',
        borderRadius: '2px',
        letterSpacing: '0',
        userSelect: 'none',
        transition: 'background 0.1s, transform 0.05s',
      },
        '.cm-marker-stamp--marked': {
          background: 'var(--color-vivid-green)',
          color: 'var(--color-sheet)',
        },
        '.cm-marker-stamp--marked:hover': {
          background: 'var(--color-vivid-green)',
        },
      '.cm-marker-stamp:hover': {
        background: 'var(--color-cinnabar-deep)',
      },
      '.cm-marker-stamp:active': {
        transform: 'translateY(1px)',
      },
    }, { dark: false });

    view.value = new EditorView({
      state: EditorState.create({
        doc,
        extensions: [
          EditorState.readOnly.of(true),
          drawSelection(),
          EditorView.lineWrapping,
          lineNumbers(),
          themeExt,
          cmCommands.history(),
          boundaryField,
          // marked-line background (driven by RangeSet<Decoration>)
          EditorView.decorations.compute([boundaryField], (v) => boundaryLineDeco(v)),
          // marker gutter: stamp on each marked line; click toggles via store
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (gutter(boundaryGutter as any) as any),
          cmSearchMod!.search({ top: true }),
        ],
      }),
      parent: host,
    });
    rebuildSearchIndex(doc);
  }

  function replaceDoc(text: string): void {
    const v = view.value;
    if (!v) return;
    v.dispatch({
      changes: { from: 0, to: v.state.doc.length, insert: text },
    });
    rebuildSearchIndex(text);
  }

  onBeforeUnmount(destroy);

  return {
    view,
    setMarkers: (lines1based: ReadonlySet<number>) => {
      const v = view.value;
      if (!v) return;
      v.dispatch({ effects: boundaryEffect.of(new Set(lines1based)) });
    },
    scrollToLine: (line0based: number) => {
      const v = view.value;
      if (!v) return;
      // store is 0-based; CM6 is 1-based. Clamp to doc bounds.
      const safe = Math.max(1, Math.min(line0based + 1, v.state.doc.lines));
      const pos = v.state.doc.line(safe).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: cmViewEditor!.scrollIntoView(pos, { y: 'start' }),
      });
    },
    runSearch: (query: string) => {
      const v = view.value;
      if (!v || !cmSearchMod) return;
      currentQuery = query;
      rebuildSearchIndex(v.state.doc.toString());
      // Drive CM's internal highlight via setSearchQuery. Empty clears.
      v.dispatch({
        effects: cmSearchMod.setSearchQuery.of(new cmSearchMod.SearchQuery({ search: query })),
      });
    },
    nextHit: () => {
      const v = view.value;
      if (!v || !cmSearchMod || !cmViewEditor || searchHits.length === 0) return;
      searchCursor = (searchCursor + 1) % searchHits.length;
      currentHitIndex.value = searchCursor + 1;
      const pos = v.state.doc.line(searchHits[searchCursor] + 1).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: cmViewEditor!.scrollIntoView(pos, { y: 'start' }),
      });
      // Move CM's internal match-selection forward too, for visual parity.
      cmSearchMod.findNext(v);
    },
    prevHit: () => {
      const v = view.value;
      if (!v || !cmSearchMod || !cmViewEditor || searchHits.length === 0) return;
      searchCursor = (searchCursor - 1 + searchHits.length) % searchHits.length;
      currentHitIndex.value = searchCursor + 1;
      const pos = v.state.doc.line(searchHits[searchCursor] + 1).from;
      v.dispatch({
        selection: { anchor: pos },
        effects: cmViewEditor!.scrollIntoView(pos, { y: 'start' }),
      });
      cmSearchMod.findPrevious(v);
    },
    hitCount,
    currentHitIndex,
    replaceDoc,
    destroy,
    mount,
  } as unknown as UseParseEditorApi;
}








