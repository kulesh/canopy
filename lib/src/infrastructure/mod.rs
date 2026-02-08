pub mod cache;
pub mod coverage;
pub mod git;
pub mod llm;
pub mod mapping_policy;
pub mod persistence;
pub mod repo_discovery;
pub mod workspace;

pub use cache::InferenceCache;
pub use coverage::{parse_lcov, CoverageMap, CoverageStats};
pub use git::{
    blame_for_file, collect_git_signals, compare_branches, BranchDiffSummary, GitSignals,
};
pub use llm::{provider_from_env, LlmCompletion, LlmProvider, ModelInfo, ProviderSelection};
pub use mapping_policy::{
    collect_source_tree_snapshot, is_source_file, normalize_path, parse_policy_response,
    policy_index, read_purpose_file, validate_policy, C4MappingPolicy, FileMappingRule,
    SourceTreeSnapshot,
};
pub use persistence::PersistenceStore;
pub use repo_discovery::{
    discover_repository, map_repository_architecture, map_repository_architecture_with_policy,
    map_repository_architecture_with_policy_and_progress,
    map_repository_architecture_with_progress, RepoMapProgress,
};
pub use workspace::{load_workspace, merge_workspace_graphs};
