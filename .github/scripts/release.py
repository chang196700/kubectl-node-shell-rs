"""Prepare releases without changing the committed Cargo version or creating commits."""

import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[2]
PACKAGE = "kubectl-node-shell"
VERSION = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"


def run(*args, cwd=ROOT):
    return subprocess.run(
        args, cwd=cwd, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()


def package_version(root=ROOT):
    return tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
        "package"
    ]["version"]


def inject_version(version, root=ROOT):
    if not re.fullmatch(VERSION, version):
        raise ValueError(f"Invalid release version: {version}")
    manifest = root / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8")
    text, count = re.subn(
        r'(?m)^version = "[^"]+"$', f'version = "{version}"', text, count=1
    )
    if count != 1:
        raise ValueError(f"Cannot find the package version in {manifest}")
    manifest.write_text(text, encoding="utf-8", newline="\n")

    lock = root / "Cargo.lock"
    text = lock.read_text(encoding="utf-8")
    text, count = re.subn(
        rf'(?m)(^name = "{re.escape(PACKAGE)}"\nversion = ")[^"]+("$)',
        lambda match: f"{match[1]}{version}{match[2]}",
        text,
    )
    if count != 1:
        raise ValueError(f"Cannot find {PACKAGE} in {lock}")
    lock.write_text(text, encoding="utf-8", newline="\n")
    if package_version(root) != version:
        raise ValueError("Cargo version injection failed")


def local_tag(version):
    tag = f"v{version}"
    existing = run("git", "tag", "--list", tag)
    if existing:
        if run("git", "rev-parse", f"{tag}^{{commit}}") != run(
            "git", "rev-parse", "HEAD"
        ):
            raise ValueError(f"{tag} already points to a different commit")
    else:
        run("git", "tag", tag, "HEAD")
    return tag


def prepare():
    head = run("git", "rev-parse", "HEAD")
    tags = [
        tag
        for tag in run(
            "git", "tag", "--merged", "HEAD", "--sort=-version:refname"
        ).splitlines()
        if re.fullmatch(f"v{VERSION}", tag)
    ]
    previous = tags[0] if tags else None

    # A retry after pushing the tag must finish that release, not bump again.
    if previous and run("git", "rev-parse", f"{previous}^{{commit}}") == head:
        version = previous[1:]
        inject_version(version)
    else:
        inject_version(previous[1:] if previous else "0.1.0")
        if previous:
            # release-plz's git-only mode reads Cargo.toml at the old tag. Since
            # that file deliberately contains 0.0.0, supply a local baseline
            # with the real tag version through --registry-manifest-path.
            # These changes exist only in temporary working directories.
            with tempfile.TemporaryDirectory(
                prefix="node-shell-release-", dir=os.environ.get("RUNNER_TEMP")
            ) as directory:
                temporary = Path(directory).resolve()
                baseline = temporary / "baseline"
                run("git", "worktree", "add", "--detach", str(baseline), previous)
                try:
                    inject_version(previous[1:], baseline)
                    config = temporary / "update.toml"
                    config.write_text(
                        (ROOT / "release-plz.toml")
                        .read_text(encoding="utf-8")
                        .replace("git_only = true", "git_only = false", 1),
                        encoding="utf-8",
                    )
                    run(
                        "release-plz", "update", "--allow-dirty",
                        "--config", str(config),
                        "--registry-manifest-path", str(baseline / "Cargo.toml"),
                    )
                finally:
                    run("git", "worktree", "remove", "--force", str(baseline))
        else:
            run("release-plz", "update", "--allow-dirty")

        version = package_version()
        if previous and version == previous[1:]:
            # Every main push is releasable, including CI/docs-only changes
            # which release-plz may exclude from the packaged crate's diff.
            major, minor, patch = map(int, version.split("."))
            version = f"{major}.{minor}.{patch + 1}"
            inject_version(version)
        if previous and tuple(map(int, version.split("."))) <= tuple(
            map(int, previous[1:].split("."))
        ):
            raise ValueError(f"Release version {version} must exceed {previous}")

    if run("git", "rev-parse", "HEAD") != head:
        raise ValueError("Release preparation must not create or change commits")
    tag = local_tag(version)
    print(f"Prepared {tag} at {head}")
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"version={version}\ntag={tag}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("prepare")
    inject = commands.add_parser("inject")
    inject.add_argument("version")
    tag = commands.add_parser("tag")
    tag.add_argument("version")
    args = parser.parse_args()
    if args.command == "prepare":
        prepare()
    elif args.command == "inject":
        inject_version(args.version)
    else:
        if not re.fullmatch(VERSION, args.version):
            parser.error("Expected a stable MAJOR.MINOR.PATCH version")
        local_tag(args.version)


if __name__ == "__main__":
    main()
