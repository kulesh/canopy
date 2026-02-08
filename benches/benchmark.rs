//! Crude startup benchmark for architecture mapping.

use std::path::Path;
use std::time::Instant;

use canopy_lib::infrastructure::{discover_repository, map_repository_architecture};

fn main() {
    let repo_path = Path::new(".");
    let start = Instant::now();

    let repository = discover_repository(repo_path).expect("discover repository");
    let graph = map_repository_architecture(&repository).expect("map architecture");

    let elapsed = start.elapsed();
    println!("Mapped {} nodes in {:?}", graph.nodes.len(), elapsed);
}
