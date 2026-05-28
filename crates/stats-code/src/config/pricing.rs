use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Model pricing
// ---------------------------------------------------------------------------
//
// 历史上这个文件还提供 `estimate_session_cost_usd` / `ChatUsageTotals` /
// `SavedChatSession` 三件，给 chat REPL 计算 token 用量与持久化会话用。
// chat REPL 已被移除（见 git 历史 cleanup/remove-chat-repl 分支），那三件
// 一起删干净；`ModelPricing` 是 `StatsCodeSettings::pricing` 的元素类型，
// 属于 settings.json on-disk schema 的一部分（向后兼容旧用户配置文件），
// 因此保留。

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelPricing {
    pub(crate) input_per_million_usd: f64,
    pub(crate) output_per_million_usd: f64,
}
