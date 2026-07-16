use super::{ProjectDiscoveryOptions, discover_project_sources};
use serde_json::json;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const WARMUP_SAMPLES: usize = 3;
const MEASURED_SAMPLES: usize = 20;
const ORDINARY_FILE_NOISE: usize = 4_000;

#[test]
#[ignore = "dedicated release-mode discovery performance harness"]
fn discovery_performance_reports_and_enforces_1_10_40_project_budgets() {
    let multiplier = budget_multiplier();
    for (projects, base_budget_micros) in [(1, 50_000_u64), (10, 75_000), (40, 125_000)] {
        let fixture = DiscoveryFixture::new(projects);
        for _ in 0..WARMUP_SAMPLES {
            fixture.scan();
        }
        let mut samples = (0..MEASURED_SAMPLES)
            .map(|_| fixture.measure_micros())
            .collect::<Vec<_>>();
        samples.sort_unstable();
        let p50 = percentile(&samples, 50);
        let p95 = percentile(&samples, 95);
        let maximum = *samples.last().expect("benchmark samples");
        let budget_micros = base_budget_micros.saturating_mul(multiplier);
        println!(
            "{}",
            json!({
                "benchmark": "stackctl_v8_project_discovery",
                "projects": projects,
                "ordinary_file_noise": ORDINARY_FILE_NOISE,
                "warmup_samples": WARMUP_SAMPLES,
                "measured_samples": MEASURED_SAMPLES,
                "p50_micros": p50,
                "p95_micros": p95,
                "max_micros": maximum,
                "budget_micros": budget_micros,
                "budget_multiplier": multiplier,
            })
        );
        assert!(
            p95 <= budget_micros,
            "{projects}-project discovery p95 {p95}us exceeds {budget_micros}us budget"
        );
    }
}

fn budget_multiplier() -> u64 {
    std::env::var("STACKCTL_DISCOVERY_BUDGET_MULTIPLIER")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| (1..=10).contains(value))
        .unwrap_or(1)
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = samples
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(samples.len().saturating_sub(1));
    samples[index]
}

struct DiscoveryFixture {
    root: PathBuf,
    projects: usize,
}

impl DiscoveryFixture {
    fn new(projects: usize) -> Self {
        let root = temporary_directory(&format!("discovery-benchmark-{projects}"));
        for index in 0..projects {
            let project = root.join(format!("project-{index:02}"));
            std::fs::create_dir(&project).expect("benchmark project directory");
            std::fs::write(
                project.join(".stackctl.yaml"),
                format!(
                    "schema_version: 8\nproject: project-{index:02}\nservices:\n  app:\n    preset: laravel\n"
                ),
            )
            .expect("benchmark project configuration");
        }
        let noise = root.join("ordinary-files");
        std::fs::create_dir(&noise).expect("ordinary-file noise directory");
        for index in 0..ORDINARY_FILE_NOISE {
            std::fs::write(noise.join(format!("artifact-{index:04}")), b"irrelevant")
                .expect("ordinary-file noise");
        }

        Self { root, projects }
    }

    fn scan(&self) {
        let report = discover_project_sources(
            std::slice::from_ref(&self.root),
            ProjectDiscoveryOptions::bounded_defaults(),
        )
        .expect("benchmark discovery");
        assert_eq!(report.sources().len(), self.projects);
        assert!(report.issues().is_empty());
    }

    fn measure_micros(&self) -> u64 {
        let started = Instant::now();
        self.scan();
        u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

impl Drop for DiscoveryFixture {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.root));
    }
}

fn temporary_directory(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("stackctl-{name}-{}-{unique}", std::process::id()));
    std::fs::create_dir(&path).expect("benchmark root");
    path
}
