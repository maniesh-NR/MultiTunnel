import subprocess
import sys
from pathlib import Path

MEMORY_FILE = Path("PROJECT_MEMORY.md")
NUM_COMMITS = 3  # Number of recent commits to pull diffs for

def get_recent_git_diffs():
    """Fetches recent commit metadata and diff patches using git log.

    S18 FIX: PROJECT_MEMORY.md is EXCLUDED from the embedded diffs. The
    old script embedded its own previous section (each run's output is
    committed into the file, so the next diff contained it) — the file
    doubled every cycle and crossed GitHub's 100 MB per-file limit
    (81 MB → 134 MB on 2026-11-21, push rejected). Long-form history
    lives in harness/memory/LEDGER.md; the elision is noted per diff.
    """
    try:
        # Run git log with patch output for the last N commits
        result = subprocess.run(
            [
                "git", "log", 
                f"-n{NUM_COMMITS}", 
                "--pretty=format:### Commit: %h - %s (%an, %ad)", 
                "--date=short", 
                "-p",
                "--", ".",
                ":(exclude)PROJECT_MEMORY.md",
            ],
            capture_output=True,
            text=True,
            check=True,
            encoding="utf-8",
            # Some history (pre-.gitattributes asset commits) embeds raw
            # .rgba bytes in diffs; never let that break the refresh.
            errors="replace"
        )
        return result.stdout.strip() + "\n\n( PROJECT_MEMORY.md's own diff is elided here — the durable\n  sections above are hand-maintained; see harness/memory/LEDGER.md\n  for the full stage history. )"
    except subprocess.CalledProcessError as e:
        print(f"[!] Error running git command: {e}")
        return ""
    except FileNotFoundError:
        print("[!] Git is not installed or not found in system PATH.")
        return ""

def update_project_memory():
    diffs = get_recent_git_diffs()
    if not diffs:
        print("[!] No git history or changes found to update.")
        return

    section_header = "## Recent Progress & Git Activity"
    new_section = f"{section_header}\n\n```diff\n{diffs}\n```\n\n"

    # Read existing memory file if it exists
    if MEMORY_FILE.exists():
        content = MEMORY_FILE.read_text(encoding="utf-8")
        
        # If the recent activity section already exists, replace it
        if section_header in content:
            parts = content.split(section_header)
            # Find the end of this section (assume next '##' header or end of file)
            remainder = parts[1]
            next_header_idx = remainder.find("\n## ")
            if next_header_idx != -1:
                remaining_content = remainder[next_header_idx:]
            else:
                remaining_content = ""
            
            updated_content = parts[0].rstrip() + "\n\n" + new_section + remaining_content.lstrip()
        else:
            # Prepend right below the main title
            updated_content = content + "\n\n" + new_section
    else:
        # Initialize a new memory file if none exists
        updated_content = f"# Project Memory & State\n\n{new_section}"

    MEMORY_FILE.write_text(updated_content, encoding="utf-8")
    print(f"[✓] Successfully updated {MEMORY_FILE} with the last {NUM_COMMITS} commit diffs.")


def rebuild_memory_graph():
    """Rebuild the agentic memory graph store (S14 protocol). The store is
    the RAG source for `query_memory.py`; keeping it fresh on every ritual
    run means all agents always retrieve current state."""
    script = Path(__file__).parent / "harness" / "memory" / "build_graph.py"
    if not script.exists():
        return
    result = subprocess.run(
        [sys.executable, str(script), "--quiet"],
        capture_output=True, text=True, encoding="utf-8")
    if result.returncode != 0:
        print(f"[!] Memory graph rebuild FAILED:\n{result.stderr.strip()}")
        sys.exit(1)
    print("[✓] Memory graph store rebuilt (harness/memory/store/).")


if __name__ == "__main__":
    update_project_memory()
    rebuild_memory_graph()
