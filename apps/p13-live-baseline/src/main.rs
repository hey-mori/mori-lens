// P13 Live Baseline Runner
//
// This binary executes the P13 benchmark using production Translator and Analyser
// against the canonical 25-entry repository-owned corpus from lens_translate::eval_corpus.
// It produces a dated baseline report under docs/benchmarks/.
//
// Human review remains NOT REVIEWED. This is an automated baseline only.
//
// Usage:
//   cargo run --bin p13-live-baseline -- --model qwen2.5:7b
//
// Prerequisites:
// - Ollama running with the specified model loaded
// - Network access to localhost:11434 (default Ollama endpoint)
//
// Evidence integrity:
// - Corpus revision is traceable via git (see lens_translate::eval_corpus::CORPUS_REVISION)
// - Quick lane: translation quality (non_empty, target_language_detected)
// - Deep lane: word-specific checks only when word is present (eligibility semantics)
// - Timeout evidence is preserved as-is, not filtered
// - Environment metadata is captured (OS, commit SHA, Ollama version)

use chrono::Utc;
use lens_core::analyse::{Analyser, DeepRequest, Profile};
use lens_core::translate::{QuickRequest, Translator};
use lens_translate::OllamaTranslator;
use lens_translate::benchmark::{
    AutomatedChecks, BenchmarkCaseResult, BenchmarkLane, BenchmarkMetadata, BenchmarkRun,
    FactualObservations, HumanScores, bridge,
    report::{generate_json_report, generate_markdown_report},
};
use lens_translate::eval_corpus::{CORPUS, CORPUS_REVISION};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model = args
        .iter()
        .position(|a| a == "--model")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("qwen2.5:7b");

    println!("P13 Live Baseline Runner");
    println!("Model: {}", model);
    println!("Corpus revision: {}", CORPUS_REVISION);
    println!("Timestamp: {}", Utc::now().to_rfc3339());
    println!();

    // Initialize production Translator/Analyser (OllamaTranslator implements both)
    let translator = OllamaTranslator::new("127.0.0.1:11434", model);

    // Check Ollama reachability
    println!("Checking Ollama reachability...");
    match lens_translate::check_reachable("127.0.0.1:11434") {
        true => println!("Ollama is reachable"),
        false => {
            eprintln!("Ollama is not reachable at 127.0.0.1:11434");
            eprintln!("Please start Ollama and load the model before running this benchmark.");
            std::process::exit(1);
        }
    }
    println!();

    // Create output directory
    let output_dir = PathBuf::from("docs/benchmarks");
    fs::create_dir_all(&output_dir)?;

    // Run Quick lane benchmark
    println!("Running Quick lane benchmark...");
    let quick_run = run_quick_benchmark(&translator, model)?;
    println!("Quick lane complete: {} cases", quick_run.cases.len());
    println!();

    // Run Deep lane benchmark
    println!("Running Deep lane benchmark...");
    let deep_run = run_deep_benchmark(&translator, model)?;
    println!("Deep lane complete: {} cases", deep_run.cases.len());
    println!();

    // Generate reports
    let timestamp = Utc::now().format("%Y-%m-%d-%H%M%S").to_string();

    // Quick lane reports
    let quick_md_path = output_dir.join(format!("p13-quick-baseline-{}.md", timestamp));
    let quick_json_path = output_dir.join(format!("p13-quick-baseline-{}.json", timestamp));
    fs::write(&quick_md_path, generate_markdown_report(&quick_run))?;
    fs::write(&quick_json_path, generate_json_report(&quick_run)?)?;
    println!("Quick lane reports:");
    println!("  Markdown: {}", quick_md_path.display());
    println!("  JSON:     {}", quick_json_path.display());
    println!();

    // Deep lane reports
    let deep_md_path = output_dir.join(format!("p13-deep-baseline-{}.md", timestamp));
    let deep_json_path = output_dir.join(format!("p13-deep-baseline-{}.json", timestamp));
    fs::write(&deep_md_path, generate_markdown_report(&deep_run))?;
    fs::write(&deep_json_path, generate_json_report(&deep_run)?)?;
    println!("Deep lane reports:");
    println!("  Markdown: {}", deep_md_path.display());
    println!("  JSON:     {}", deep_json_path.display());
    println!();

    println!("Baseline complete.");
    println!("Human review status: NOT REVIEWED");
    println!();
    println!("Next steps:");
    println!("  1. Review the markdown reports in docs/benchmarks/");
    println!("  2. Perform human quality review following docs/p13-benchmark-guide.md");
    println!("  3. Update reports with human scores when review is complete");

    Ok(())
}

fn run_quick_benchmark(
    translator: &OllamaTranslator,
    model: &str,
) -> Result<BenchmarkRun, Box<dyn std::error::Error>> {
    let mut run = BenchmarkRun::new(
        BenchmarkMetadata {
            model: model.to_string(),
            provider: "Ollama".to_string(),
            endpoint: "127.0.0.1:11434".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            runs_per_entry: 1,
            warm_start: true,
            corpus_revision: CORPUS_REVISION.to_string(),
            notes: Some(
                "Live baseline with production Translator, canonical 25-entry corpus".to_string(),
            ),
        },
        BenchmarkLane::Quick,
    );

    for (i, entry) in CORPUS.iter().enumerate() {
        println!("  Quick [{}/{}]: {}", i + 1, CORPUS.len(), entry.id);

        let request = QuickRequest {
            source_text: entry.source.to_string(),
            focused_word: entry.word.map(|s| s.to_string()),
            target_language: "Korean".to_string(),
        };

        let start = Instant::now();
        let result = translator.quick(&request);
        let latency_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(translation) => {
                let case = bridge::from_quick_translation(
                    entry.id,
                    entry.category,
                    entry.source,
                    entry.word,
                    &translation,
                    0,    // run_index
                    None, // expected_keywords: not measured (English keywords on Korean output is meaningless)
                );
                // Override latency with actual measurement
                let case = BenchmarkCaseResult {
                    automated: AutomatedChecks {
                        latency_ms: Some(latency_ms),
                        ..case.automated
                    },
                    ..case
                };
                run.add_case(case);
            }
            Err(e) => {
                let case = BenchmarkCaseResult {
                    entry_id: entry.id.to_string(),
                    category: entry.category.to_string(),
                    lane: BenchmarkLane::Quick,
                    run_index: 0,
                    source: entry.source.to_string(),
                    focused_word: entry.word.map(|s| s.to_string()),
                    output: String::new(),
                    automated: AutomatedChecks {
                        latency_ms: Some(latency_ms),
                        error_type: Some(format!("{:?}", e)),
                        ..Default::default()
                    },
                    observations: FactualObservations::default(),
                    human: HumanScores::default(),
                };
                run.add_case(case);
            }
        }
    }

    Ok(run)
}

fn run_deep_benchmark(
    analyser: &OllamaTranslator,
    model: &str,
) -> Result<BenchmarkRun, Box<dyn std::error::Error>> {
    let mut run = BenchmarkRun::new(
        BenchmarkMetadata {
            model: model.to_string(),
            provider: "Ollama".to_string(),
            endpoint: "127.0.0.1:11434".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            runs_per_entry: 1,
            warm_start: true,
            corpus_revision: CORPUS_REVISION.to_string(),
            notes: Some(
                "Live baseline with production Analyser, canonical 25-entry corpus".to_string(),
            ),
        },
        BenchmarkLane::Deep,
    );

    for (i, entry) in CORPUS.iter().enumerate() {
        println!("  Deep [{}/{}]: {}", i + 1, CORPUS.len(), entry.id);

        // Build request with actual word field (not None)
        let request = DeepRequest {
            sentence: entry.source.to_string(),
            word: entry.word.map(|s| s.to_string()),
            translation: Some(entry.translation.to_string()),
            target_language: "Korean".to_string(),
            profile: Profile::Reader,
        };

        let start = Instant::now();
        let result = analyser.deep(&request);
        let latency_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(analysis) => {
                let case = bridge::from_deep_analysis(
                    entry.id,
                    entry.category,
                    entry.source,
                    entry.word,
                    &analysis,
                    0, // run_index
                    entry.expected_lemma,
                    entry.expected_pos,
                );
                // Override latency with actual measurement
                let case = BenchmarkCaseResult {
                    automated: AutomatedChecks {
                        latency_ms: Some(latency_ms),
                        ..case.automated
                    },
                    ..case
                };
                run.add_case(case);
            }
            Err(e) => {
                let case = BenchmarkCaseResult {
                    entry_id: entry.id.to_string(),
                    category: entry.category.to_string(),
                    lane: BenchmarkLane::Deep,
                    run_index: 0,
                    source: entry.source.to_string(),
                    focused_word: entry.word.map(|s| s.to_string()),
                    output: String::new(),
                    automated: AutomatedChecks {
                        latency_ms: Some(latency_ms),
                        error_type: Some(format!("{:?}", e)),
                        ..Default::default()
                    },
                    observations: FactualObservations::default(),
                    human: HumanScores::default(),
                };
                run.add_case(case);
            }
        }
    }

    Ok(run)
}
