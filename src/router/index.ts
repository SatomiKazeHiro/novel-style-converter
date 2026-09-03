import { createRouter, createWebHistory } from 'vue-router';
import Models from '../views/Models.vue';
import Library from '../views/Library.vue';
import Upload from '../views/Upload.vue';
import DataAsset from '../views/DataAsset.vue';
import ParseWizard from '../views/parse.vue';
import Transform from '../views/Transform.vue';
import Prompts from '../views/Prompts.vue';
import AiCalls from '../views/AiCalls.vue';
import Overview from '../views/Overview.vue';
import { findDataAssetByUpload } from '../ipc/commands';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/uploads' },
    { path: '/library', redirect: '/uploads' },
    { path: '/uploads', component: Library, meta: { libraryPage: 'uploads' } },
    { path: '/data-assets', component: Library, meta: { libraryPage: 'data-assets' } },
    { path: '/transformations', component: Library, meta: { libraryPage: 'transformations' } },
    {
      path: '/library/upload/:uploadId',
      component: Upload,
      name: 'upload',
    },
    {
      path: '/library/upload/:uploadId/parse',
      component: ParseWizard,
      name: 'parse-wizard',
    },
    {
      path: '/library/data/:dataAssetId',
      component: DataAsset,
      name: 'data-asset',
    },
    {
      path: '/library/transform/:chapterId',
      component: Transform,
      name: 'transform',
    },
    {
      path: '/library/transformation/:tnId',
      name: 'transformation-detail',
      component: () => import('../views/TransformationNovelDetail.vue'),
      props: true,
    },
    { path: '/overview', component: Overview },
    { path: '/prompts', component: Prompts },
    { path: '/ai-calls', component: AiCalls },
    { path: '/models', component: Models },
  ],
});

/// Old /library/:uploadId/{clean,chapters,preview} paths are routed to the right page
/// depending on whether a data_asset already exists for that upload.
router.beforeEach(async (to) => {
  const oldMatch = to.path.match(/^\/library\/(\d+)\/(clean|chapters|preview)$/);
  if (oldMatch) {
    const uploadId = Number(oldMatch[1]);
    const daIds = await findDataAssetByUpload(uploadId);
    return daIds.length > 0 ? `/library/data/${daIds[0]}` : `/library/upload/${uploadId}`;
  }
});

export default router;