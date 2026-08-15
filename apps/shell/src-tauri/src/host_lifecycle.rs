//! Host 生命周期状态机。
//!
//! `DshManager` 保留实际 `Child` 与线程调度，这里负责 generation、运行阶段与
//! 看门狗重启预算，避免状态判断散落在消息循环里。

/// 连续自动重启预算。超过后不再重启，转 failed。
pub const DEFAULT_AUTO_RESTART_LIMIT: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPhase {
    Stopped,
    Starting,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartDecision {
    /// 当前 generation 已失效，忽略该事件。
    Ignore,
    Restart {
        attempt: u32,
        limit: u32,
    },
    Fail,
}

#[derive(Debug)]
pub struct HostLifecycle {
    generation: u64,
    phase: HostPhase,
    auto_restarts: u32,
    max_auto_restarts: u32,
}

impl HostLifecycle {
    pub fn new(max_auto_restarts: u32) -> Self {
        Self {
            generation: 0,
            phase: HostPhase::Stopped,
            auto_restarts: 0,
            max_auto_restarts,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    #[cfg(test)]
    pub fn phase(&self) -> HostPhase {
        self.phase
    }

    #[cfg(test)]
    pub fn auto_restarts(&self) -> u32 {
        self.auto_restarts
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    /// 手动 Start / 安装完成后清零连续重启计数。
    pub fn reset_restarts(&mut self) {
        self.auto_restarts = 0;
    }

    /// 进入一次新的启动，返回本次 generation。
    pub fn begin_start(&mut self) -> u64 {
        self.generation += 1;
        self.phase = HostPhase::Starting;
        self.generation
    }

    /// 让当前 generation 立即失效（清理旧子进程、停止/退出时使用）。
    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.phase = HostPhase::Stopped;
    }

    /// 接受一条 Ready 消息；只接受当前 generation，并返回是否进入 Ready。
    pub fn accept_ready(&mut self, generation: u64) -> bool {
        if !self.is_current(generation) {
            return false;
        }
        self.phase = HostPhase::Ready;
        true
    }

    /// 处理运行中进程的意外退出。当前没有 running 阶段或 generation 过期时忽略。
    pub fn on_unexpected_exit(&mut self, generation: u64) -> RestartDecision {
        if !self.is_current(generation) {
            return RestartDecision::Ignore;
        }
        if !matches!(self.phase, HostPhase::Starting | HostPhase::Ready) {
            return RestartDecision::Ignore;
        }
        self.phase = HostPhase::Stopped;
        if self.auto_restarts < self.max_auto_restarts {
            self.auto_restarts += 1;
            RestartDecision::Restart {
                attempt: self.auto_restarts,
                limit: self.max_auto_restarts,
            }
        } else {
            RestartDecision::Fail
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_generates_new_generation() {
        let mut lifecycle = HostLifecycle::new(3);
        let generation = lifecycle.begin_start();
        assert_eq!(generation, 1);
        assert_eq!(lifecycle.phase(), HostPhase::Starting);
        assert!(lifecycle.is_current(generation));
    }

    #[test]
    fn accepts_ready_only_for_current_generation() {
        let mut lifecycle = HostLifecycle::new(3);
        let generation = lifecycle.begin_start();
        assert!(lifecycle.accept_ready(generation));
        assert_eq!(lifecycle.phase(), HostPhase::Ready);
        assert!(!lifecycle.accept_ready(generation + 1));
    }

    #[test]
    fn invalidate_ignores_stale_messages() {
        let mut lifecycle = HostLifecycle::new(3);
        let generation = lifecycle.begin_start();
        lifecycle.invalidate();
        assert!(!lifecycle.accept_ready(generation));
        assert_eq!(
            lifecycle.on_unexpected_exit(generation),
            RestartDecision::Ignore
        );
    }

    #[test]
    fn restarts_within_budget_then_fails() {
        let mut lifecycle = HostLifecycle::new(2);
        let mut generation = lifecycle.begin_start();

        for attempt in 1..=2 {
            assert_eq!(
                lifecycle.on_unexpected_exit(generation),
                RestartDecision::Restart { attempt, limit: 2 }
            );
            generation = lifecycle.begin_start();
        }

        assert_eq!(
            lifecycle.on_unexpected_exit(generation),
            RestartDecision::Fail
        );
        assert_eq!(lifecycle.auto_restarts(), 2);
    }

    #[test]
    fn manual_start_resets_restart_budget() {
        let mut lifecycle = HostLifecycle::new(1);
        let mut generation = lifecycle.begin_start();
        assert!(matches!(
            lifecycle.on_unexpected_exit(generation),
            RestartDecision::Restart { .. }
        ));

        lifecycle.reset_restarts();
        generation = lifecycle.begin_start();
        assert!(matches!(
            lifecycle.on_unexpected_exit(generation),
            RestartDecision::Restart { attempt: 1, .. }
        ));
    }

    #[test]
    fn ignores_exit_when_not_running() {
        let mut lifecycle = HostLifecycle::new(3);
        assert_eq!(lifecycle.on_unexpected_exit(1), RestartDecision::Ignore);
    }
}
