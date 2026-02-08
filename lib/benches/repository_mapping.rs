use std::fs;
use std::hint::black_box;
use std::path::Path;
use std::time::Duration;

use canopy_lib::infrastructure::{discover_repository, map_repository_architecture};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tempfile::TempDir;

#[derive(Clone, Copy)]
struct Scenario {
    name: &'static str,
    containers: usize,
    files_per_container: usize,
}

impl Scenario {
    fn source_files(self) -> usize {
        self.containers * self.files_per_container
    }
}

fn write_fixture_repo(root: &Path, scenario: Scenario) {
    fs::create_dir_all(root.join("src")).expect("create src");
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname='fixture'\nversion='0.1.0'\n",
    )
    .expect("write pyproject");

    for container_index in 0..scenario.containers {
        let container_name = format!("domain_{container_index:02}");
        let container_dir = root.join("src").join(&container_name);
        fs::create_dir_all(&container_dir).expect("create container dir");
        fs::write(container_dir.join("__init__.py"), "").expect("write init");

        for file_index in 0..scenario.files_per_container {
            let next_file = if file_index + 1 < scenario.files_per_container {
                format!("file_{:03}", file_index + 1)
            } else {
                "shared".to_string()
            };
            let code = format!(
                "from .{next_file} import run as next_run\n\n\
                 def run(input_value: int) -> int:\n\
                     value = input_value + {container_index} + {file_index}\n\
                     return next_run(value)\n"
            );
            let file_path = container_dir.join(format!("file_{file_index:03}.py"));
            fs::write(file_path, code).expect("write source");
        }

        fs::write(
            container_dir.join("shared.py"),
            "def run(value: int) -> int:\n    return value\n",
        )
        .expect("write shared");
    }
}

fn benchmark_repository_mapping(c: &mut Criterion) {
    let scenarios = [
        Scenario {
            name: "small",
            containers: 3,
            files_per_container: 12,
        },
        Scenario {
            name: "medium",
            containers: 6,
            files_per_container: 24,
        },
        Scenario {
            name: "large",
            containers: 10,
            files_per_container: 40,
        },
    ];

    let fixtures: Vec<(Scenario, TempDir)> = scenarios
        .into_iter()
        .map(|scenario| {
            let temp_dir = tempfile::tempdir().expect("tempdir");
            write_fixture_repo(temp_dir.path(), scenario);
            (scenario, temp_dir)
        })
        .collect();

    let mut group = c.benchmark_group("repository_mapping");
    group.warm_up_time(Duration::from_millis(700));
    group.measurement_time(Duration::from_secs(6));
    group.sample_size(20);

    for (scenario, temp_dir) in &fixtures {
        let repository = discover_repository(temp_dir.path()).expect("discover");
        group.throughput(Throughput::Elements(scenario.source_files() as u64));
        group.bench_with_input(
            BenchmarkId::new("discover_and_map", scenario.name),
            scenario,
            |b, _| {
                b.iter(|| {
                    let graph = map_repository_architecture(black_box(&repository)).expect("map");
                    black_box(graph.nodes.len())
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_repository_mapping);
criterion_main!(benches);
