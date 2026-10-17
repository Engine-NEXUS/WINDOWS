//! The one plan waiting for a spoken yes (docs/features/140, P3). Process-global, expires on its
//! own, and is replaced (never queued) by the next plan, so a stale "yes" can never confirm an
//! older request than the one that was read back last.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::files::Plan;

/// Longer than this and the read-back is stale; the user has to ask again.
pub const TTL: Duration = Duration::from_secs(120);

struct Held {
    plan: Plan,
    at: Instant,
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Held>> {
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

/// Hold `plan` for the confirmation. Replaces any earlier one.
pub fn hold(plan: Plan) {
    *lock() = Some(Held { plan, at: Instant::now() });
}

/// Take the plan if it is still fresh. It is consumed: a second "yes" finds nothing.
pub fn take() -> Option<Plan> {
    let held = lock().take()?;
    (held.at.elapsed() <= TTL).then_some(held.plan)
}

pub fn clear() {
    *lock() = None;
}

#[cfg(test)]
pub static TEST_GUARD: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn backdate(by: Duration) {
    if let Some(h) = lock().as_mut() {
        h.at = Instant::now() - by;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::files::{plan, Ctx, Request};
    use crate::action_kernel::paths::test_dir;
    use crate::action_kernel::policy::test_env;
    use crate::action_kernel::{Origin, Tool};

    fn is_holding() -> bool {
        lock().as_ref().map_or(false, |h| h.at.elapsed() <= TTL)
    }

    fn a_plan(tag: &str) -> Plan {
        let root = test_dir(tag);
        let (env, policy, roots) = test_env(&root);
        let f = roots[0].join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let data = env.data_dir.clone();
        let ctx = Ctx { dir: &data, policy: &policy, env: &env, roots: &roots, now: 1, phrase: "p", origin: Origin::UserVoice };
        plan(&ctx, &Request { tool: Tool::Delete, sources: vec![f], dest_dir: None, new_name: None }).unwrap()
    }

    #[test]
    fn a_plan_is_consumed_once_and_replaced_by_the_next() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        hold(a_plan("pend1"));
        assert!(is_holding());
        hold(a_plan("pend2"));
        assert!(take().is_some());
        assert!(take().is_none(), "a second yes finds nothing");
        assert!(!is_holding());
    }

    #[test]
    fn a_stale_plan_is_not_returned() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        hold(a_plan("pend3"));
        backdate(TTL + Duration::from_secs(1));
        assert!(!is_holding());
        assert!(take().is_none());
    }
}
