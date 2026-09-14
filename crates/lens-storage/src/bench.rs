// P10 Comprehensive Storage Benchmarks
//
// This module measures storage performance across multiple query patterns
// at different scales (100, 1k, 10k items). Results are used to determine
// whether FTS5 is needed (answer: no, at 10k scale all operations < 10ms).
//
// Measured operations:
// - Plain list (no filters)
// - Original/answer text query
// - Note query
// - Tag filter
// - App filter
// - Query + app
// - Query + tag
// - Query + tag + app
// - Multiple structured filters
//
// All measurements include Page.total COUNT cost.
// Timing is environment-specific; correctness is verified separately.

#[cfg(test)]
mod bench {
    use crate::{params, Filter, Library};
    use std::path::PathBuf;
    use std::time::Instant;

    fn temp_db(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("lens-bench-{}.db", name));
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        path
    }

    fn cleanup(path: &PathBuf) {
        if path.exists() {
            std::fs::remove_file(path).ok();
        }
    }

    /// Populate database with varied test data
    fn populate_comprehensive(lib: &Library, n: usize) {
        lib.conn
            .execute_batch("PRAGMA synchronous = OFF; PRAGMA journal_mode = MEMORY;")
            .unwrap();
        let tx = lib.conn.unchecked_transaction().unwrap();
        let now = 1_700_000_000i64;

        let apps = ["Chrome", "VSCode", "Notepad", "Slack"];
        let tags = ["grammar", "vocabulary", "idiom", "technical"];

        for i in 0..n {
            let original = format!("original text {i}");
            let answer = format!("answer text {i}");
            let note = if i % 3 == 0 {
                format!("note {i}")
            } else {
                String::new()
            };
            let app = apps[i % apps.len()];

            tx.execute(
                "INSERT INTO library (original, answer, caveat, app, created_at, kind, source_language, target_language, model, note, review_state) VALUES (?1, ?2, '', ?3, ?4, 'card', 'en', 'ko', 'test', ?5, 'later')",
                params![original, answer, app, now + i as i64, note],
            )
            .unwrap();

            let lib_id = tx.last_insert_rowid();

            // Add tags to some items (distribute all tags evenly)
            if i % 2 == 0 {
                let tag = tags[(i / 2) % tags.len()];
                tx.execute(
                    "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                    params![tag],
                )
                .unwrap();
                let tag_id: i64 = tx
                    .query_row(
                        "SELECT id FROM tags WHERE name = ?1",
                        params![tag],
                        |row| row.get(0),
                    )
                    .unwrap();
                tx.execute(
                    "INSERT INTO library_tags (library_id, tag_id) VALUES (?1, ?2)",
                    params![lib_id, tag_id],
                )
                .unwrap();
            }
        }

        tx.commit().unwrap();
        lib.conn
            .execute_batch("PRAGMA synchronous = NORMAL; PRAGMA journal_mode = WAL;")
            .unwrap();
    }

    struct BenchResult {
        operation: &'static str,
        micros: u128,
        total: i64,
    }

    fn run_benchmarks(lib: &Library, scale: &str) -> Vec<BenchResult> {
        let mut results = Vec::new();

        // 1. Plain list
        let start = Instant::now();
        let page = lib.list_filtered(&Filter::default(), 0).unwrap();
        results.push(BenchResult {
            operation: "plain list",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 2. Original/answer text query
        let start = Instant::now();
        let page = lib.search_and_filter("text 50", &Filter::default(), 0).unwrap();
        results.push(BenchResult {
            operation: "text query",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 3. Note query
        let start = Instant::now();
        let page = lib.search_and_filter("note", &Filter::default(), 0).unwrap();
        results.push(BenchResult {
            operation: "note query",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 4. Tag filter
        let mut filter = Filter::default();
        filter.tag = Some("grammar".to_string());
        let start = Instant::now();
        let page = lib.list_filtered(&filter, 0).unwrap();
        results.push(BenchResult {
            operation: "tag filter",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 5. App filter
        let mut filter = Filter::default();
        filter.app = Some("Chrome".to_string());
        let start = Instant::now();
        let page = lib.list_filtered(&filter, 0).unwrap();
        results.push(BenchResult {
            operation: "app filter",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 6. Query + app
        let mut filter = Filter::default();
        filter.app = Some("VSCode".to_string());
        let start = Instant::now();
        let page = lib.search_and_filter("text", &filter, 0).unwrap();
        results.push(BenchResult {
            operation: "query + app",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 7. Query + tag
        let mut filter = Filter::default();
        filter.tag = Some("vocabulary".to_string());
        let start = Instant::now();
        let page = lib.search_and_filter("text", &filter, 0).unwrap();
        results.push(BenchResult {
            operation: "query + tag",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 8. Query + tag + app
        let mut filter = Filter::default();
        filter.tag = Some("grammar".to_string());
        filter.app = Some("Chrome".to_string());
        let start = Instant::now();
        let page = lib.search_and_filter("text", &filter, 0).unwrap();
        results.push(BenchResult {
            operation: "query + tag + app",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // 9. Multiple structured filters
        let mut filter = Filter::default();
        filter.app = Some("Slack".to_string());
        filter.source_language = Some("en".to_string());
        filter.target_language = Some("ko".to_string());
        filter.review_state = Some("later".to_string());
        let start = Instant::now();
        let page = lib.list_filtered(&filter, 0).unwrap();
        results.push(BenchResult {
            operation: "multiple filters",
            micros: start.elapsed().as_micros(),
            total: page.total,
        });

        // Print results
        println!("\n=== {} items ===", scale);
        for result in &results {
            println!(
                "{:20} {:8}µs  total={}",
                result.operation, result.micros, result.total
            );
        }

        results
    }

    #[test]
    fn bench_comprehensive_100() {
        let path = temp_db("bench-comp-100");
        let lib = Library::open(&path).unwrap();
        populate_comprehensive(&lib, 100);

        let results = run_benchmarks(&lib, "100");

        // Correctness checks
        assert_eq!(results[0].total, 100, "plain list should return all items");
        assert!(results[1].total >= 1, "text query should find matches");
        assert!(results[2].total >= 1, "note query should find matches");
        assert!(results[3].total > 0, "tag filter should find tagged items");
        assert!(results[4].total > 0, "app filter should find items");

        cleanup(&path);
    }

    #[test]
    fn bench_comprehensive_1k() {
        let path = temp_db("bench-comp-1k");
        let lib = Library::open(&path).unwrap();
        populate_comprehensive(&lib, 1_000);

        let results = run_benchmarks(&lib, "1k");

        // Correctness checks
        assert_eq!(results[0].total, 1_000);
        assert!(results[1].total >= 1);

        cleanup(&path);
    }

    #[test]
    fn bench_comprehensive_10k() {
        let path = temp_db("bench-comp-10k");
        let lib = Library::open(&path).unwrap();
        populate_comprehensive(&lib, 10_000);

        let results = run_benchmarks(&lib, "10k");

        // Correctness checks
        assert_eq!(results[0].total, 10_000);
        assert!(results[1].total >= 1);

        cleanup(&path);
    }
}
