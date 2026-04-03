#!/usr/bin/env python3
"""Generate a standardized benchmark corpus for document parser comparison.

Creates files of known sizes and complexity levels for reproducible benchmarking.
Corpus structure:
  benchmark-corpus/
    small/        100 files, ~1KB each (simple text)
    medium/       100 files, ~10KB each (structured with headings, lists, tables)
                  100 .html, 100 .csv, 100 .md files
    large/        50 files, ~100KB each (complex text reports)
                  50 .html files with complex tables
    mixed-format/ 50 each of .txt, .csv, .html, .md (200 total)

Usage:
    python scripts/generate_benchmark_corpus.py [--output benchmark-corpus]
"""

import argparse
import os
import random
import string
import textwrap

# Seed for reproducibility
RANDOM_SEED = 42

# --- Vocabulary for realistic-ish content ---

WORDS = (
    "the of and to a in is it you that he was for on are with as I his they be "
    "at one have this from or had by not word but what some we can out other were "
    "all there when up use your how said an each she which do their time if will "
    "way about many then them write would like so these her long make thing see him "
    "two has look more day could go come did number sound no most people my over know "
    "water than call first who may down side been now find head stand own page should "
    "country found answer school grow study still learn plant cover food sun four between "
    "state keep eye never last let thought city tree cross farm hard start might story "
    "saw far sea draw left late run while press close night real life few north open "
    "seem together next white children begin got walk example ease paper group always "
    "music those both mark often letter until mile river car feet care second book carry "
    "took science eat room friend began idea fish mountain stop once base hear horse cut "
    "sure watch color face wood main enough plain girl usual young ready above ever red "
    "list though feel talk bird soon body dog family direct pose leave song measure door "
    "product black short numeral class wind question happen complete ship area half rock "
    "order fire south problem piece told knew pass since top whole king space heard best "
    "hour better true during hundred five remember step early hold west ground interest "
    "reach fast verb sing listen six table travel less morning ten simple several vowel "
    "toward war lay against pattern slow center love person money serve appear road map "
    "rain rule govern pull cold notice voice unit power town fine certain fly fall lead "
    "cry dark machine note wait plan figure star box noun field rest correct able pound "
    "done beauty drive stood contain front teach week final gave green oh quick develop "
    "ocean warm free minute strong special mind behind clear tail produce fact street inch "
    "multiply nothing course stay wheel full force blue object decide surface deep moon "
    "island foot system busy test record boat common gold possible plane age dry wonder "
    "laugh thousand ago ran check game shape equate hot miss brought heat snow tire bring "
    "yes distant fill east paint language among grand ball yet wave drop heart am present "
    "heavy dance engine position arm wide sail material size vary settle speak weight "
    "general ice matter circle pair include divide syllable felt grand ball surface deep"
).split()

HEADINGS = [
    "Executive Summary", "Introduction", "Background", "Methodology",
    "Data Collection", "Analysis", "Results", "Discussion",
    "Key Findings", "Recommendations", "Implementation Plan",
    "Risk Assessment", "Budget Overview", "Timeline", "Stakeholder Analysis",
    "Technical Architecture", "Performance Metrics", "Quality Assurance",
    "Deployment Strategy", "Monitoring and Evaluation", "Conclusion",
    "Appendix A: Raw Data", "Appendix B: Survey Results",
    "Appendix C: Technical Specifications", "References",
]

CODE_SNIPPETS = [
    'def process_data(input_path, output_path):\n    """Process raw data and write results."""\n    with open(input_path) as f:\n        data = json.load(f)\n    results = [transform(item) for item in data]\n    with open(output_path, "w") as f:\n        json.dump(results, f, indent=2)\n    return len(results)',
    "SELECT u.name, u.email, COUNT(o.id) AS order_count\nFROM users u\nLEFT JOIN orders o ON u.id = o.user_id\nWHERE u.created_at > '2024-01-01'\nGROUP BY u.id\nHAVING COUNT(o.id) > 5\nORDER BY order_count DESC\nLIMIT 100;",
    'import numpy as np\nimport pandas as pd\n\ndef compute_statistics(df: pd.DataFrame) -> dict:\n    return {\n        "mean": df["value"].mean(),\n        "std": df["value"].std(),\n        "median": df["value"].median(),\n        "q1": df["value"].quantile(0.25),\n        "q3": df["value"].quantile(0.75),\n    }',
    'async function fetchData(endpoint: string): Promise<ApiResponse> {\n  const response = await fetch(`${BASE_URL}/${endpoint}`, {\n    headers: { Authorization: `Bearer ${token}` },\n  });\n  if (!response.ok) throw new HttpError(response.status);\n  return response.json();\n}',
]

CSV_HEADERS = [
    ["id", "name", "email", "department", "salary"],
    ["timestamp", "sensor_id", "temperature", "humidity", "pressure"],
    ["order_id", "customer", "product", "quantity", "total"],
    ["date", "region", "category", "revenue", "units_sold"],
]

DEPARTMENTS = ["Engineering", "Marketing", "Sales", "Finance", "HR", "Operations", "Legal", "Support"]
PRODUCTS = ["Widget A", "Widget B", "Gadget Pro", "Sensor X", "Module Y", "Kit Z", "Pack S", "Unit Q"]
REGIONS = ["North America", "Europe", "Asia Pacific", "Latin America", "Middle East", "Africa"]
CATEGORIES = ["Electronics", "Software", "Services", "Hardware", "Consulting", "Training"]


def rand_sentence(rng, min_words=8, max_words=25):
    length = rng.randint(min_words, max_words)
    words = [rng.choice(WORDS) for _ in range(length)]
    words[0] = words[0].capitalize()
    return " ".join(words) + "."


def rand_paragraph(rng, min_sentences=3, max_sentences=8):
    count = rng.randint(min_sentences, max_sentences)
    return " ".join(rand_sentence(rng) for _ in range(count))


def rand_name(rng):
    first = "".join(rng.choices(string.ascii_lowercase, k=rng.randint(4, 8)))
    last = "".join(rng.choices(string.ascii_lowercase, k=rng.randint(5, 10)))
    return f"{first.capitalize()} {last.capitalize()}"


def rand_email(rng, name):
    parts = name.lower().split()
    domain = rng.choice(["example.com", "test.org", "corp.net", "demo.io"])
    return f"{parts[0]}.{parts[1]}@{domain}"


# === Generators ===

def generate_small_txt(rng, target_bytes=1024):
    """~1KB plain text paragraphs."""
    paragraphs = []
    while True:
        p = rand_paragraph(rng)
        paragraphs.append(p)
        text = "\n\n".join(paragraphs)
        if len(text.encode("utf-8")) >= target_bytes:
            break
    return "\n\n".join(paragraphs)


def generate_medium_txt(rng, target_bytes=10240):
    """~10KB structured text with headings, lists, paragraphs."""
    sections = []
    headings = list(HEADINGS)
    rng.shuffle(headings)

    for heading in headings:
        section = f"{heading}\n{'=' * len(heading)}\n\n"
        section += rand_paragraph(rng, 4, 10) + "\n\n"

        # Add a bullet list
        num_bullets = rng.randint(3, 7)
        for _ in range(num_bullets):
            section += f"  - {rand_sentence(rng, 6, 15)}\n"
        section += "\n"

        section += rand_paragraph(rng, 3, 6) + "\n\n"
        sections.append(section)

        text = "\n".join(sections)
        if len(text.encode("utf-8")) >= target_bytes:
            break

    return "\n".join(sections)


def generate_medium_html(rng, target_bytes=10240):
    """~10KB HTML with tables, headings, lists, paragraphs."""
    parts = [
        "<!DOCTYPE html>",
        "<html><head><title>Benchmark Document</title></head>",
        "<body>",
    ]

    headings = list(HEADINGS)
    rng.shuffle(headings)

    for heading in headings:
        parts.append(f"<h2>{heading}</h2>")
        parts.append(f"<p>{rand_paragraph(rng, 4, 8)}</p>")

        # Table
        rows = rng.randint(5, 15)
        cols = rng.randint(3, 5)
        parts.append("<table border='1'>")
        parts.append("<tr>" + "".join(f"<th>Column {c+1}</th>" for c in range(cols)) + "</tr>")
        for _ in range(rows):
            cells = "".join(
                f"<td>{rng.choice(WORDS)} {rng.randint(1, 9999)}</td>" for _ in range(cols)
            )
            parts.append(f"<tr>{cells}</tr>")
        parts.append("</table>")

        # Unordered list
        parts.append("<ul>")
        for _ in range(rng.randint(3, 6)):
            parts.append(f"<li>{rand_sentence(rng, 5, 12)}</li>")
        parts.append("</ul>")

        text = "\n".join(parts)
        if len(text.encode("utf-8")) >= target_bytes:
            break

    parts.append("</body></html>")
    return "\n".join(parts)


def generate_medium_csv(rng, target_rows=200):
    """~10KB CSV with 200 rows, 5 columns."""
    header_set = rng.choice(CSV_HEADERS)
    lines = [",".join(header_set)]

    for i in range(1, target_rows + 1):
        if header_set[0] == "id":
            name = rand_name(rng)
            row = [
                str(i),
                name,
                rand_email(rng, name),
                rng.choice(DEPARTMENTS),
                str(rng.randint(40000, 180000)),
            ]
        elif header_set[0] == "timestamp":
            row = [
                f"2024-{rng.randint(1,12):02d}-{rng.randint(1,28):02d}T{rng.randint(0,23):02d}:{rng.randint(0,59):02d}:00Z",
                f"sensor_{rng.randint(1, 50):03d}",
                f"{rng.uniform(15.0, 45.0):.2f}",
                f"{rng.uniform(20.0, 95.0):.2f}",
                f"{rng.uniform(990.0, 1040.0):.2f}",
            ]
        elif header_set[0] == "order_id":
            name = rand_name(rng)
            row = [
                f"ORD-{rng.randint(10000, 99999)}",
                name,
                rng.choice(PRODUCTS),
                str(rng.randint(1, 100)),
                f"{rng.uniform(10.0, 5000.0):.2f}",
            ]
        else:
            row = [
                f"2024-{rng.randint(1,12):02d}-{rng.randint(1,28):02d}",
                rng.choice(REGIONS),
                rng.choice(CATEGORIES),
                f"{rng.uniform(1000.0, 500000.0):.2f}",
                str(rng.randint(10, 10000)),
            ]
        lines.append(",".join(row))

    return "\n".join(lines)


def generate_medium_md(rng, target_bytes=10240):
    """~10KB Markdown with code blocks, headings, lists."""
    parts = [f"# Benchmark Report {rng.randint(1, 999)}\n"]

    headings = list(HEADINGS)
    rng.shuffle(headings)

    for heading in headings:
        level = rng.choice(["##", "###"])
        parts.append(f"{level} {heading}\n")
        parts.append(rand_paragraph(rng, 3, 8) + "\n")

        # Bullet list
        for _ in range(rng.randint(3, 6)):
            parts.append(f"- {rand_sentence(rng, 5, 15)}")
        parts.append("")

        # Maybe a code block
        if rng.random() < 0.5:
            snippet = rng.choice(CODE_SNIPPETS)
            lang = rng.choice(["python", "sql", "typescript", "python"])
            parts.append(f"```{lang}")
            parts.append(snippet)
            parts.append("```\n")

        # Maybe a table
        if rng.random() < 0.4:
            cols = rng.randint(3, 5)
            header = "| " + " | ".join(f"Col {c+1}" for c in range(cols)) + " |"
            sep = "| " + " | ".join("---" for _ in range(cols)) + " |"
            parts.append(header)
            parts.append(sep)
            for _ in range(rng.randint(3, 8)):
                row = "| " + " | ".join(
                    f"{rng.choice(WORDS)} {rng.randint(1, 999)}" for _ in range(cols)
                ) + " |"
                parts.append(row)
            parts.append("")

        parts.append(rand_paragraph(rng, 2, 5) + "\n")

        text = "\n".join(parts)
        if len(text.encode("utf-8")) >= target_bytes:
            break

    return "\n".join(parts)


def generate_large_txt(rng, target_bytes=102400):
    """~100KB text simulating a long report."""
    sections = []
    headings = list(HEADINGS)

    while True:
        rng.shuffle(headings)
        for heading in headings:
            section = f"\n{'=' * 72}\n{heading.upper()}\n{'=' * 72}\n\n"
            # Multiple paragraphs per section
            for _ in range(rng.randint(5, 12)):
                section += rand_paragraph(rng, 5, 12) + "\n\n"
            # Numbered list
            for j in range(1, rng.randint(4, 10)):
                section += f"  {j}. {rand_sentence(rng, 8, 20)}\n"
            section += "\n"
            sections.append(section)

            text = "\n".join(sections)
            if len(text.encode("utf-8")) >= target_bytes:
                return "\n".join(sections)

    return "\n".join(sections)


def generate_large_html(rng, target_bytes=102400):
    """~100KB HTML with complex nested tables and structure."""
    parts = [
        "<!DOCTYPE html>",
        '<html lang="en"><head><meta charset="utf-8"><title>Complex Report</title></head>',
        "<body>",
        "<h1>Comprehensive Analysis Report</h1>",
    ]

    headings = list(HEADINGS)
    section_num = 0

    while True:
        rng.shuffle(headings)
        for heading in headings:
            section_num += 1
            parts.append(f"<h2>{section_num}. {heading}</h2>")

            for _ in range(rng.randint(3, 6)):
                parts.append(f"<p>{rand_paragraph(rng, 5, 10)}</p>")

            # Large table
            rows = rng.randint(15, 40)
            cols = rng.randint(4, 7)
            parts.append('<table border="1" cellpadding="4">')
            parts.append("<thead><tr>")
            for c in range(cols):
                parts.append(f"<th>Header {c+1}</th>")
            parts.append("</tr></thead><tbody>")
            for _ in range(rows):
                parts.append("<tr>")
                for _ in range(cols):
                    parts.append(f"<td>{rng.choice(WORDS)} {rng.randint(1, 99999)}</td>")
                parts.append("</tr>")
            parts.append("</tbody></table>")

            # Nested lists
            parts.append("<ol>")
            for _ in range(rng.randint(4, 8)):
                parts.append(f"<li>{rand_sentence(rng, 8, 20)}")
                parts.append("<ul>")
                for _ in range(rng.randint(2, 4)):
                    parts.append(f"<li>{rand_sentence(rng, 5, 12)}</li>")
                parts.append("</ul></li>")
            parts.append("</ol>")

            text = "\n".join(parts)
            if len(text.encode("utf-8")) >= target_bytes:
                parts.append("</body></html>")
                return "\n".join(parts)

    parts.append("</body></html>")
    return "\n".join(parts)


def write_file(directory, filename, content):
    os.makedirs(directory, exist_ok=True)
    filepath = os.path.join(directory, filename)
    with open(filepath, "w", encoding="utf-8") as f:
        f.write(content)
    return filepath


def main():
    parser = argparse.ArgumentParser(description="Generate benchmark corpus for parser comparison")
    parser.add_argument("--output", default="benchmark-corpus", help="Output directory (default: benchmark-corpus)")
    parser.add_argument("--seed", type=int, default=RANDOM_SEED, help="Random seed for reproducibility")
    args = parser.parse_args()

    rng = random.Random(args.seed)
    base = args.output

    print(f"Generating benchmark corpus in '{base}/' with seed={args.seed}")
    total_files = 0
    total_bytes = 0

    # --- small/ ---
    print("  small/ (100 x ~1KB .txt) ...", end=" ", flush=True)
    for i in range(1, 101):
        content = generate_small_txt(rng)
        path = write_file(os.path.join(base, "small"), f"doc_{i:03d}.txt", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- medium/ .txt ---
    print("  medium/ (100 x ~10KB .txt) ...", end=" ", flush=True)
    for i in range(1, 101):
        content = generate_medium_txt(rng)
        write_file(os.path.join(base, "medium"), f"doc_{i:03d}.txt", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- medium/ .html ---
    print("  medium/ (100 x ~10KB .html) ...", end=" ", flush=True)
    for i in range(1, 101):
        content = generate_medium_html(rng)
        write_file(os.path.join(base, "medium"), f"doc_{i:03d}.html", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- medium/ .csv ---
    print("  medium/ (100 x ~10KB .csv) ...", end=" ", flush=True)
    for i in range(1, 101):
        content = generate_medium_csv(rng)
        write_file(os.path.join(base, "medium"), f"doc_{i:03d}.csv", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- medium/ .md ---
    print("  medium/ (100 x ~10KB .md) ...", end=" ", flush=True)
    for i in range(1, 101):
        content = generate_medium_md(rng)
        write_file(os.path.join(base, "medium"), f"doc_{i:03d}.md", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- large/ .txt ---
    print("  large/ (50 x ~100KB .txt) ...", end=" ", flush=True)
    for i in range(1, 51):
        content = generate_large_txt(rng)
        write_file(os.path.join(base, "large"), f"doc_{i:03d}.txt", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- large/ .html ---
    print("  large/ (50 x ~100KB .html) ...", end=" ", flush=True)
    for i in range(1, 51):
        content = generate_large_html(rng)
        write_file(os.path.join(base, "large"), f"doc_{i:03d}.html", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    # --- mixed-format/ ---
    print("  mixed-format/ (50 each of .txt, .csv, .html, .md) ...", end=" ", flush=True)
    mixed_dir = os.path.join(base, "mixed-format")
    for i in range(1, 51):
        # .txt (~5KB)
        content = generate_medium_txt(rng, target_bytes=5120)
        write_file(mixed_dir, f"doc_{i:03d}.txt", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1

        # .csv
        content = generate_medium_csv(rng, target_rows=150)
        write_file(mixed_dir, f"doc_{i:03d}.csv", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1

        # .html
        content = generate_medium_html(rng, target_bytes=5120)
        write_file(mixed_dir, f"doc_{i:03d}.html", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1

        # .md
        content = generate_medium_md(rng, target_bytes=5120)
        write_file(mixed_dir, f"doc_{i:03d}.md", content)
        total_bytes += len(content.encode("utf-8"))
        total_files += 1
    print("done")

    total_mb = total_bytes / (1024 * 1024)
    print(f"\nCorpus generated: {total_files} files, {total_mb:.1f} MB total")
    print(f"Location: {os.path.abspath(base)}/")


if __name__ == "__main__":
    main()
