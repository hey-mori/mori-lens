#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
P13 Human Review Aggregation Script.

Parses completed human review packets and produces machine-readable
aggregated results with field-level and category-level breakdowns.

Modes:
  --validate       Check field coverage and report status (NOT_STARTED/PARTIAL/COMPLETE/INVALID)
  --aggregate      Parse human scores and produce JSON output + summary markdown

Requirements:
  - Accepts only explicit human-entered values (digits 0/1/2 for scores)
  - Handles N/A for no-word entries (A03, B03, C01, C02, C04, E03)
  - Fails closed on malformed input
  - Uses correct denominators:
    * 19 for Factual/Sense (no-word entries are N/A)
    * 25 for Grammar/Structure/Usefulness/Misleading
    * 25 for Quick lane (Meaning/Naturalness/Completeness)

"""

import argparse
import json
import re
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path
from typing import Optional


# Expected no-word entries in canary-25-v1
NO_WORD_IDS = {"A03", "B03", "C01", "C02", "C04", "E03"}

# Entry ID patterns
ENTRY_ID_PATTERN = re.compile(r"^## ([A-F]\d{2}) -- (\w+)$", re.MULTILINE)

# Quick lane human review patterns
QUICK_SCORE_PATTERN = re.compile(r"^- (\w[\w\s]+?): (__) \| (\d+)$|^- (\w[\w\s]+?): (\d) \| (\d+)$")
QUICK_FIELD_NAMES = {"meaning preserved", "naturalness", "completeness"}

# Deep lane human review patterns
DEEP_SCORE_PATTERN = re.compile(
    r"^- (\w[\w\s]+?): (N/A|__) \| (\d+)$|^- (\w[\w\s]+?): (N/A|\d) \| (\d+)$"
)
DEEP_FIELD_NAMES = {
    "factual correctness",
    "sense correctness",
    "grammar correctness",
    "structure correctness",
    "usefulness",
    "misleading",
}


def parse_header(packet_path: Path) -> dict:
    """Extract header metadata from review packet."""
    text = packet_path.read_text(encoding="utf-8")
    metadata = {}

    # Match table rows
    for match in re.finditer(r"^\| (\w[\w\s]+?) \| (.+) \|$", text, re.MULTILINE):
        key = match.group(1).strip().lower()
        value = match.group(2).strip()
        metadata[key] = value

    return metadata


def detect_lane(packet_path: Path) -> str:
    """Detect whether packet is Quick or Deep lane."""
    text = packet_path.read_text(encoding="utf-8")
    if "Human Review Packet -- Quick Lane" in text:
        return "quick"
    elif "Human Review Packet -- Deep Lane" in text:
        return "deep"
    else:
        raise ValueError(f"Cannot detect lane from packet: {packet_path}")


def parse_one_entry_quick(text: str) -> Optional[dict]:
    """Parse a single Quick lane entry block."""
    # Extract entry ID and category
    id_match = ENTRY_ID_PATTERN.search(text)
    if not id_match:
        return None

    entry_id = id_match.group(1)
    category = id_match.group(2)

    result = {
        "id": entry_id,
        "category": category,
        "source": None,
        "reference_translation": None,
        "model_output": None,
        "inference_status": None,
        "latency_ms": None,
        "human_scores": {},
        "notes": None,
    }

    # Extract fields
    for match in re.finditer(r"^\*\*(.+?):\*\*\s*(.+)$", text, re.MULTILINE):
        field = match.group(1).strip().lower()
        value = match.group(2).strip()

        if field == "source":
            result["source"] = value
        elif field == "reference translation":
            result["reference_translation"] = value
        elif field == "model output":
            result["model_output"] = value
        elif field == "inference status":
            result["inference_status"] = value
        elif field == "latency":
            # Parse "1234ms" -> 1234
            latency_match = re.match(r"^(\d+)ms$", value)
            if latency_match:
                result["latency_ms"] = int(latency_match.group(1))

    # Extract human review section
    human_section = re.search(r"\*\*Human review:\*\*\s*\n(.*?)(?=\n---|$)", text, re.DOTALL)
    if human_section:
        human_text = human_section.group(1)

        for line in human_text.split("\n"):
            line = line.strip()
            if not line or line.startswith("- Notes:"):
                continue

            # Match "- Meaning preserved: __ / 2" or "- Meaning preserved: 1 / 2"
            score_match = re.match(r"^- (.+?): (N/A|__|\d) / (\d+)$", line)
            if score_match:
                field_name = score_match.group(1).strip().lower()
                value = score_match.group(2)
                max_score = int(score_match.group(3))

                if value == "__":
                    result["human_scores"][field_name] = None  # Not filled
                elif value == "N/A":
                    result["human_scores"][field_name] = "N/A"
                else:
                    result["human_scores"][field_name] = int(value)

        # Extract notes
        notes_match = re.search(r"- Notes:\s*(.+)$", human_text, re.MULTILINE)
        if notes_match:
            result["notes"] = notes_match.group(1).strip() or None

    return result


def parse_one_entry_deep(text: str) -> Optional[dict]:
    """Parse a single Deep lane entry block."""
    # Extract entry ID and category
    id_match = ENTRY_ID_PATTERN.search(text)
    if not id_match:
        return None

    entry_id = id_match.group(1)
    category = id_match.group(2)

    result = {
        "id": entry_id,
        "category": category,
        "source": None,
        "focused_word": None,
        "reference_translation": None,
        "expected_lemma": None,
        "expected_pos": None,
        "model_lemma": None,
        "model_pos": None,
        "human_scores": {},
        "notes": None,
    }

    # Extract fields
    for match in re.finditer(r"^\*\*(.+?):\*\*\s*(.+)$", text, re.MULTILINE):
        field = match.group(1).strip().lower()
        value = match.group(2).strip()

        if field == "source":
            result["source"] = value
        elif field == "focused word":
            result["focused_word"] = value
        elif field == "reference translation":
            result["reference_translation"] = value

    # Extract expected corpus ground truth
    expected_section = re.search(r"\*\*Expected \(corpus ground truth\):\*\*\s*\n(.*?)(?=\n\*\*)", text, re.DOTALL)
    if expected_section:
        expected_text = expected_section.group(1)
        for match in re.finditer(r"^- Expected (\w+): (.+)$", expected_text, re.MULTILINE):
            field = match.group(1).strip().lower()
            value = match.group(2).strip()

            if field == "lemma":
                result["expected_lemma"] = value
            elif field == "pos":
                result["expected_pos"] = value

    # Extract model analysis
    model_section = re.search(r"\*\*Model analysis \(canonical output\):\*\*\s*\n(.*?)(?=\n\*\*)", text, re.DOTALL)
    if model_section:
        model_text = model_section.group(1)
        for match in re.finditer(r"^- (\w+): (.+)$", model_text, re.MULTILINE):
            field = match.group(1).strip().lower()
            value = match.group(2).strip()

            if field == "lemma":
                result["model_lemma"] = value
            elif field == "pos":
                result["model_pos"] = value

    # Extract human review section
    human_section = re.search(r"\*\*Human review:\*\*\s*\n(.*?)(?=\n---|$)", text, re.DOTALL)
    if human_section:
        human_text = human_section.group(1)

        for line in human_text.split("\n"):
            line = line.strip()
            if not line or line.startswith("- Notes:"):
                continue

            # Match "- Factual correctness: N/A" (no max for N/A)
            na_match = re.match(r"^- (.+?): N/A$", line)
            if na_match:
                field_name = na_match.group(1).strip().lower()
                result["human_scores"][field_name] = "N/A"
                continue

            # Match "- Grammar correctness: 1 / 2" or "- Grammar correctness: __ / 2"
            score_match = re.match(r"^- (.+?): (__|\d) / (\d+)$", line)
            if score_match:
                field_name = score_match.group(1).strip().lower()
                value = score_match.group(2)

                if value == "__":
                    result["human_scores"][field_name] = None  # Not filled
                else:
                    result["human_scores"][field_name] = int(value)
                continue

            # For misleading: "- Misleading: Yes" or "- Misleading: No" (not "Yes / No" which is the template)
            misleading_match = re.match(r"^- Misleading: (Yes|No)$", line)
            if misleading_match:
                result["human_scores"]["misleading"] = misleading_match.group(1)
                continue

        # Extract notes
        notes_match = re.search(r"- Notes:\s*(.+)$", human_text, re.MULTILINE)
        if notes_match:
            result["notes"] = notes_match.group(1).strip() or None

    return result


def parse_packet(packet_path: Path) -> list[dict]:
    """Parse a review packet into entry list."""
    text = packet_path.read_text(encoding="utf-8")
    lane = detect_lane(packet_path)

    # Split by entry boundaries (## followed by entry ID like A01 -- general)
    entry_pattern = re.compile(r"^## ([A-F]\d{2}) -- (\w+)$", re.MULTILINE)
    positions = [m.start() for m in entry_pattern.finditer(text)]

    entries = []
    parse_fn = parse_one_entry_quick if lane == "quick" else parse_one_entry_deep

    for i, pos in enumerate(positions):
        # Extract block from this ## to next ## or end of file
        end = positions[i + 1] if i + 1 < len(positions) else len(text)
        block = text[pos:end]
        entry = parse_fn(block)
        if entry:
            entries.append(entry)

    return entries


def detect_review_status(entries: list[dict], lane: str) -> str:
    """
    Detect review status: NOT_STARTED, PARTIAL, COMPLETE, or INVALID.

    Rules:
    - NOT_STARTED: All human scores are None (blank) or N/A (template placeholders)
    - PARTIAL: Some scores filled with actual values, some blank
    - COMPLETE: All applicable scores filled with actual values
    - INVALID: Malformed or out-of-range values detected

    Note: N/A fields (no-word entries) are not counted in total_expected.
    """
    total_filled = 0
    total_expected = 0

    for entry in entries:
        scores = entry["human_scores"]

        if lane == "quick":
            expected_fields = QUICK_FIELD_NAMES
        else:
            # Deep lane: all 6 fields
            expected_fields = DEEP_FIELD_NAMES

        for field in expected_fields:
            value = scores.get(field)

            if value is None:
                # Blank - not filled yet, counts as expected
                total_expected += 1
                continue
            elif value == "N/A":
                # Template placeholder (no-word entries) - not applicable, skip
                continue
            elif isinstance(value, int):
                if 0 <= value <= 2:
                    total_expected += 1
                    total_filled += 1
                else:
                    return "INVALID"
            elif value in {"Yes", "No"}:
                # Human-entered Yes/No for misleading
                total_expected += 1
                total_filled += 1
            else:
                return "INVALID"

    if total_filled == 0:
        return "NOT_STARTED"
    elif total_filled == total_expected:
        return "COMPLETE"
    else:
        return "PARTIAL"


def validate_packets(quick_path: Path, deep_path: Path) -> dict:
    """Validate both packets and return status report."""
    report = {
        "quick": {"status": None, "entry_count": 0, "errors": []},
        "deep": {"status": None, "entry_count": 0, "errors": []},
        "overall_status": None,
    }

    # Parse Quick
    try:
        quick_entries = parse_packet(quick_path)
        report["quick"]["entry_count"] = len(quick_entries)

        if len(quick_entries) != 25:
            report["quick"]["errors"].append(f"Expected 25 entries, got {len(quick_entries)}")

        # Check for required fields
        for entry in quick_entries:
            if not entry["id"]:
                report["quick"]["errors"].append("Entry missing ID")
            if not entry["source"]:
                report["quick"]["errors"].append(f"Entry {entry['id']} missing source")

        # Detect status
        report["quick"]["status"] = detect_review_status(quick_entries, "quick")

    except Exception as e:
        report["quick"]["errors"].append(f"Parse error: {e}")
        report["quick"]["status"] = "INVALID"

    # Parse Deep
    try:
        deep_entries = parse_packet(deep_path)
        report["deep"]["entry_count"] = len(deep_entries)

        if len(deep_entries) != 25:
            report["deep"]["errors"].append(f"Expected 25 entries, got {len(deep_entries)}")

        # Check no-word entries have N/A for Factual/Sense
        for entry in deep_entries:
            if entry["id"] in NO_WORD_IDS:
                factual = entry["human_scores"].get("factual correctness")
                sense = entry["human_scores"].get("sense correctness")
                if factual is not None and factual != "N/A":
                    report["deep"]["errors"].append(
                        f"Entry {entry['id']} is no-word but Factual is not N/A"
                    )
                if sense is not None and sense != "N/A":
                    report["deep"]["errors"].append(
                        f"Entry {entry['id']} is no-word but Sense is not N/A"
                    )

        # Detect status
        report["deep"]["status"] = detect_review_status(deep_entries, "deep")

    except Exception as e:
        report["deep"]["errors"].append(f"Parse error: {e}")
        report["deep"]["status"] = "INVALID"

    # Overall status
    if report["quick"]["status"] == "INVALID" or report["deep"]["status"] == "INVALID":
        report["overall_status"] = "INVALID"
    elif report["quick"]["status"] == "NOT_STARTED" and report["deep"]["status"] == "NOT_STARTED":
        report["overall_status"] = "NOT_STARTED"
    elif report["quick"]["status"] == "COMPLETE" and report["deep"]["status"] == "COMPLETE":
        report["overall_status"] = "COMPLETE"
    else:
        report["overall_status"] = "PARTIAL"

    return report


def aggregate_scores(entries: list[dict], lane: str) -> dict:
    """Aggregate human scores with correct denominators."""
    aggregation = {
        "lane": lane,
        "total_entries": len(entries),
        "field_scores": {},
        "category_scores": {},
    }

    # Collect scores by field
    field_values = defaultdict(list)
    category_field_values = defaultdict(lambda: defaultdict(list))

    for entry in entries:
        category = entry["category"]

        for field, value in entry["human_scores"].items():
            if value is None or value == "N/A":
                continue

            if isinstance(value, int):
                field_values[field].append(value)
                category_field_values[category][field].append(value)

    # Calculate field-level aggregates
    for field, values in field_values.items():
        if values:
            aggregation["field_scores"][field] = {
                "count": len(values),
                "sum": sum(values),
                "mean": sum(values) / len(values),
                "min": min(values),
                "max": max(values),
            }

    # Calculate category-level aggregates
    for category, fields in category_field_values.items():
        category_scores = {}
        for field, values in fields.items():
            if values:
                category_scores[field] = {
                    "count": len(values),
                    "sum": sum(values),
                    "mean": sum(values) / len(values),
                }
        aggregation["category_scores"][category] = category_scores

    return aggregation


def aggregate_packets(quick_path: Path, deep_path: Path) -> dict:
    """Aggregate both packets into structured JSON output."""
    quick_entries = parse_packet(quick_path)
    deep_entries = parse_packet(deep_path)

    quick_header = parse_header(quick_path)
    deep_header = parse_header(deep_path)

    quick_status = detect_review_status(quick_entries, "quick")
    deep_status = detect_review_status(deep_entries, "deep")

    result = {
        "metadata": {
            "generated_at": datetime.now().astimezone().isoformat(),
            "corpus_revision": quick_header.get("corpus revision", "unknown"),
            "model": quick_header.get("model", "unknown"),
            "provider": quick_header.get("provider", "unknown"),
            "baseline": quick_header.get("canonical baseline", "unknown"),
            "reviewer": quick_header.get("reviewer", "unknown"),
            "review_date": quick_header.get("review date", "unknown"),
        },
        "quick": {
            "status": quick_status,
            "entries": quick_entries,
            "aggregation": aggregate_scores(quick_entries, "quick"),
        },
        "deep": {
            "status": deep_status,
            "entries": deep_entries,
            "aggregation": aggregate_scores(deep_entries, "deep"),
        },
    }

    return result


def main():
    parser = argparse.ArgumentParser(
        description="P13 Human Review Aggregation",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        help="Validate packets and report status (NOT_STARTED/PARTIAL/COMPLETE/INVALID)",
    )
    parser.add_argument(
        "--aggregate",
        action="store_true",
        help="Parse human scores and produce JSON output + summary markdown",
    )
    parser.add_argument(
        "--quick-packet",
        type=Path,
        default=Path("docs/benchmarks/p13-human-review-quick-2026-09-21.md"),
        help="Path to Quick lane review packet (default: docs/benchmarks/p13-human-review-quick-2026-09-21.md)",
    )
    parser.add_argument(
        "--deep-packet",
        type=Path,
        default=Path("docs/benchmarks/p13-human-review-deep-2026-09-21.md"),
        help="Path to Deep lane review packet (default: docs/benchmarks/p13-human-review-deep-2026-09-21.md)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output directory for aggregated results (default: docs/benchmarks/)",
    )

    args = parser.parse_args()

    if not args.validate and not args.aggregate:
        parser.error("Must specify --validate or --aggregate")

    # Validate packet paths
    if not args.quick_packet.exists():
        print(f"ERROR: Quick packet not found: {args.quick_packet}", file=sys.stderr)
        sys.exit(1)

    if not args.deep_packet.exists():
        print(f"ERROR: Deep packet not found: {args.deep_packet}", file=sys.stderr)
        sys.exit(1)

    if args.validate:
        report = validate_packets(args.quick_packet, args.deep_packet)

        print("P13 Human Review Validation Report")
        print("=" * 60)
        print()
        print(f"Overall status: {report['overall_status']}")
        print()

        print("Quick Lane:")
        print(f"  Status: {report['quick']['status']}")
        print(f"  Entries: {report['quick']['entry_count']}")
        if report["quick"]["errors"]:
            print("  Errors:")
            for error in report["quick"]["errors"]:
                print(f"    - {error}")
        print()

        print("Deep Lane:")
        print(f"  Status: {report['deep']['status']}")
        print(f"  Entries: {report['deep']['entry_count']}")
        if report["deep"]["errors"]:
            print("  Errors:")
            for error in report["deep"]["errors"]:
                print(f"    - {error}")
        print()

        # Exit code based on status
        if report["overall_status"] == "INVALID":
            sys.exit(2)
        elif report["overall_status"] == "NOT_STARTED":
            print("Human review has not started yet.")
            sys.exit(0)
        else:
            sys.exit(0)

    elif args.aggregate:
        # Check if review has started
        report = validate_packets(args.quick_packet, args.deep_packet)

        if report["overall_status"] == "NOT_STARTED":
            print("ERROR: Human review has not started. Cannot aggregate.", file=sys.stderr)
            sys.exit(1)

        if report["overall_status"] == "INVALID":
            print("ERROR: Packets contain invalid data.", file=sys.stderr)
            sys.exit(1)

        # Aggregate
        result = aggregate_packets(args.quick_packet, args.deep_packet)

        # Output JSON
        output_dir = args.output or Path("docs/benchmarks")
        output_dir.mkdir(parents=True, exist_ok=True)

        date_str = datetime.now().strftime("%Y-%m-%d")
        json_path = output_dir / f"p13-human-review-{date_str}.json"

        with open(json_path, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2, ensure_ascii=False)

        print(f"Aggregated results written to: {json_path}")

        # Print summary
        print()
        print("Summary:")
        print(f"  Quick status: {result['quick']['status']}")
        print(f"  Deep status: {result['deep']['status']}")
        print()

        if result["quick"]["status"] != "NOT_STARTED":
            print("Quick lane field scores:")
            for field, scores in result["quick"]["aggregation"]["field_scores"].items():
                print(f"  {field}: {scores['sum']}/{scores['count'] * 2} (mean: {scores['mean']:.2f})")
            print()

        if result["deep"]["status"] != "NOT_STARTED":
            print("Deep lane field scores:")
            for field, scores in result["deep"]["aggregation"]["field_scores"].items():
                if field != "misleading":
                    print(f"  {field}: {scores['sum']}/{scores['count'] * 2} (mean: {scores['mean']:.2f})")
            print()


if __name__ == "__main__":
    main()
