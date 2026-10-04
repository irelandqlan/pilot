# Pilot Justfile
# Modern command runner recipes for development, testing, service management, and releases.

default:
    @just --list

# ── Development & Testing ───────────────────────────────────────────────────

# Run Pilot settings GUI locally with cargo
run *args:
    cargo run -- {{args}}

# Run all unit tests
test:
    cargo test

# Fast compiler check
check:
    cargo check

# Build optimized release binary
build-release:
    cargo build --release

# ── System Installation & Service Management ────────────────────────────────

# Install Pilot binaries, services, desktop launcher, and icons
install:
    ./manage.sh install

# Uninstall Pilot binaries and user services
uninstall:
    ./manage.sh uninstall

# Inspect running services, Bluetooth connection, and input permissions
status:
    ./manage.sh status

# Restart all background daemons
restart:
    ./manage.sh restart

# Follow combined journalctl output for Pilot daemons
logs:
    ./manage.sh logs

# Stop all background daemons
stop:
    ./manage.sh stop

# Start all background daemons
start:
    ./manage.sh start

# ── Git & Branch Management ─────────────────────────────────────────────────

# Safely pull latest commits with rebase on the current branch
sync:
    git pull --rebase origin $(git rev-parse --abbrev-ref HEAD)

# Safely merge develop into master and push
merge-to-master:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "==> Checking git working directory..."
    if ! git diff-index --quiet HEAD --; then
        echo "❌ Error: You have uncommitted changes. Please commit or stash them before merging."
        exit 1
    fi

    echo "==> Running tests before merge..."
    cargo test --quiet

    if git remote | grep -q origin; then
        echo "==> Fetching latest changes from GitHub..."
        git fetch origin || true
    fi

    echo "==> Switching to master..."
    git checkout master
    if git remote | grep -q origin; then
        git pull --rebase origin master || true
    fi

    echo "==> Merging develop into master..."
    git merge develop -m "Merge branch 'develop' into master"

    if git remote | grep -q origin; then
        echo "==> Pushing master to GitHub..."
        git push origin master
    fi

    echo "==> Switching back to develop..."
    git checkout develop

    echo ""
    echo "✅ Successfully merged develop into master!"

# ── Releases ────────────────────────────────────────────────────────────────

# Create a new release (e.g.: just release 0.2.0)
release version:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "==> Preparing release v{{version}}..."

    # Ensure working tree is clean
    if ! git diff-index --quiet HEAD --; then
        echo "❌ Error: Working tree has uncommitted changes. Please commit or stash them first."
        exit 1
    fi

    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
    if [ "$CURRENT_BRANCH" = "develop" ]; then
        echo "==> Merging develop to master first..."
        just merge-to-master
        git checkout master
    elif [ "$CURRENT_BRANCH" != "master" ]; then
        echo "❌ Error: Releases must be triggered from 'develop' or 'master' branch (currently on '$CURRENT_BRANCH')."
        exit 1
    fi

    echo "==> Running test suite..."
    cargo test --quiet

    echo "==> Updating version to {{version}} in Cargo.toml..."
    sed -i -E 's/^version = "[^"]*"/version = "{{version}}"/' Cargo.toml
    cargo check --quiet

    TODAY=$(date +%Y-%m-%d)
    METAINFO="data/io.github.magnotec.Pilot.metainfo.xml"
    if [ -f "$METAINFO" ] && grep -q "<releases>" "$METAINFO"; then
        sed -i -E "s|<releases>|<releases>\n    <release version=\"{{version}}\" date=\"$TODAY\" />|" "$METAINFO"
    fi

    echo "==> Committing release bump..."
    git commit -am "chore: bump version to {{version}}"

    echo "==> Creating git tag v{{version}}..."
    git tag -a "v{{version}}" -m "Release v{{version}}"

    if git remote | grep -q origin; then
        echo "==> Pushing commit and tag to GitHub..."
        git push origin master
        git push origin "v{{version}}"
    fi

    if [ "$CURRENT_BRANCH" = "develop" ]; then
        echo "==> Updating develop with the release commit..."
        git checkout develop
        git merge master --ff-only || git merge master -m "Merge master into develop after v{{version}}"
        if git remote | grep -q origin; then
            git push origin develop
        fi
    fi

    echo ""
    echo "🎉 Release v{{version}} created!"
