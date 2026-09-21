//! 锁中毒恢复 —— 全 crate 共用的一份实现。
//!
//! ## 为什么统一在这里收口
//! `Db` 是全应用**唯一**的共享 Connection(main / worker / scheduler / recorder 共用),
//! 任何一处 `lock().expect("... poisoned")` 在 worker 里 panic 后都会让**每个**线程的
//! **每一次** `lock()` 连锁 panic —— 单次 worker panic 会升级成整个应用不可用。
//!
//! 而中毒本身不代表被保护的数据损坏:Rust 的 Mutex 中毒是**保守**语义(无法证明状态
//! 一致),但这里保护的对象有两种,都不是"Rust 侧半改状态":
//! - `rusqlite::Connection`:C 侧对象,不会因 Rust unwind 处于半改状态;真正需要原子性
//!   的部分走 `unchecked_transaction` 的 SQLite 事务,未提交事务在连接复用时由 SQLite
//!   自己回滚。
//! - `HashMap` / `Vec` / `Option<Notifier>`:即便上一次 push 到一半 panic,最坏是丢掉
//!   一条 UI 通知或一个缓存项 —— 远好过整个应用瘫痪。
//!
//! 所以:恢复并继续 > 让一处 panic 永久瘫痪全部功能。

use std::sync::{Mutex, MutexGuard};

/// 取锁;遇中毒时打日志并**恢复**继续使用,而不是 panic。
///
/// `what` 用于日志定位是哪把锁(如 `"db"` / `"provider cache"`)。
pub fn lock_recover<'a, T>(mutex: &'a Mutex<T>, what: &str) -> MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        eprintln!(
            "[sync] {what} lock poisoned(有线程持锁时 panic);已恢复继续使用。\
             若反复出现,说明该路径在持锁期间 panic,需查上游"
        );
        poisoned.into_inner()
    })
}

/// 把 `catch_unwind` 的 payload 转成可读字符串 —— 后台线程(worker / recorder writer)
/// 上报 panic 时统一用这一份,避免同一件事多处写法不同、各自踩同一个坑。
///
/// 参数收 `&Box<dyn Any + Send>`(而不是 `&(dyn Any + Send)`)是**必须**的:
/// `panic!("字面量")` 的 payload 是 boxed 的 `&'static str`;传 `&payload` 时
/// `downcast_ref::<&'static str>()` 恰好能命中。若签名写成 `&(dyn Any + Send)`,
/// 调用处的 deref coercion 会把 `&Box<dyn Any>` 先转成 `&dyn Any`,
/// 于是 `downcast_ref::<&'static str>()` 永远失败、字面量 panic 全部退化成
/// "unknown panic payload",真实原因被丢掉。
pub fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中毒后仍能取到锁并正常读写 —— 这是"单次 panic 不再瘫痪全应用"的核心保证。
    #[test]
    fn recovers_poisoned_mutex_and_keeps_data_usable() {
        let m = Mutex::new(vec![1, 2, 3]);
        // 持锁 panic → 中毒(与 worker 在持锁期间 panic 同一路径)。
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = m.lock().unwrap();
            panic!("模拟持锁 panic");
        }));
        assert!(m.is_poisoned(), "前置条件:锁已中毒");

        // 恢复路径:不 panic,且数据仍可读可写。
        let mut g = lock_recover(&m, "test");
        assert_eq!(*g, vec![1, 2, 3], "恢复后已有数据应仍可读");
        g.push(4);
        drop(g);
        assert_eq!(
            *lock_recover(&m, "test"),
            vec![1, 2, 3, 4],
            "恢复后可继续写"
        );
    }

    /// 未中毒时行为与普通 lock 一致(不引入额外语义)。
    #[test]
    fn normal_lock_is_unaffected() {
        let m = Mutex::new(7);
        assert_eq!(*lock_recover(&m, "test"), 7);
        assert!(!m.is_poisoned());
    }

    /// `panic!("字面量")` 的 payload 必须能解析出真实信息。
    ///
    /// 这是 worker 与 recorder 都曾踩过的坑(两处各写了一份 downcast,都拿不到字面量):
    /// 真实的 panic 原因被吞成 "unknown panic payload",排查时等于没有线索。
    #[test]
    fn panic_message_extracts_literal_payload() {
        let payload =
            std::panic::catch_unwind(|| panic!("真实的 panic 原因")).expect_err("应当 panic");
        let msg = panic_message(&payload);
        assert_eq!(msg, "真实的 panic 原因", "字面量 payload 应被完整取出");
        assert!(!msg.contains("unknown"), "不应退化成 unknown: {msg}");
    }

    /// 非字符串 payload(如 `panic_any(42)`)退化为占位文案,而不是 panic。
    #[test]
    fn panic_message_handles_non_string_payload() {
        let payload =
            std::panic::catch_unwind(|| std::panic::panic_any(42i32)).expect_err("应当 panic");
        assert_eq!(panic_message(&payload), "unknown panic payload");
    }
}
