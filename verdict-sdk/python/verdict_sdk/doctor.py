import os
import sys
from pathlib import Path
from typing import List, Tuple


def check_config() -> Tuple[bool, str]:
    from .config_loader import load_config

    if os.path.exists("eval.yaml"):
        try:
            load_config("eval.yaml")
            return True, "✅ eval.yaml found and valid."
        except Exception as e:
            return False, f"❌ eval.yaml found but invalid: {e}"
    else:
        return True, "⚠️ eval.yaml not found (skipping config check)."


def check_permissions() -> Tuple[bool, str]:
    eval_dir = Path(".eval")
    try:
        eval_dir.mkdir(exist_ok=True)
        test_file = eval_dir / ".perm_check"
        test_file.touch()
        test_file.unlink()
        return True, "✅ .eval/ directory is writable."
    except Exception as e:
        return False, f"❌ .eval/ directory not writable: {e}"


def check_openai() -> Tuple[bool, str]:
    try:
        import openai
    except ImportError:
        return (
            True,
            "⚠️ 'openai' package not installed. Judge features disabled. (Install with pip install verdict-sdk[openai])",
        )

    if "OPENAI_API_KEY" not in os.environ:
        return (
            False,
            "⚠️ 'openai' installed but OPENAI_API_KEY environment variable is missing.",
        )

    return True, "✅ OpenAI ready (package installed, API key set)."


def run_doctor() -> None:
    print("Verdict SDK Doctor 🩺")
    print("======================")

    checks = [
        check_config,
        check_permissions,
        check_openai,
    ]

    all_ok = True
    for check in checks:
        ok, msg = check()
        print(msg)
        if not ok and "❌" in msg:
            all_ok = False

    print("======================")
    if all_ok:
        print("Everything looks good! 🚀")
    else:
        print("Some issues found. See above.")
        sys.exit(1)


if __name__ == "__main__":
    run_doctor()
