#!/usr/bin/env bash
# Pilot Release Management Utility
# Handles creating releases, safely synchronizing branches, and rolling back failed releases.

set -euo pipefail

usage() {
    local code="${1:-0}"
    cat << 'EOF'
Usage: just release <command | version>

Release Workflow Commands:
  just release <version>        Create, tag, and publish release (e.g. 0.2.1)
  just release create <version> Explicit release creation
  just release abort [version]  Roll back an interrupted/failed release and restore branch
  just release sync             Safely merge 'develop' into 'master' and push

Examples:
  just release 0.2.1            # Tests, bumps version, commits, tags, and pushes v0.2.1
  just release abort            # Restores working tree, deletes unpushed tag, returns to develop
EOF
    exit "$code"
}

cmd_sync() {
    echo "==> Checking git working directory..."
    if ! git diff-index --quiet HEAD --; then
        echo "Error: You have uncommitted changes. Please commit or stash them before merging."
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
    echo "Successfully merged develop into master!"
}

cmd_abort() {
    local target_ver="${1:-}"
    local target_tag=""
    if [ -n "$target_ver" ]; then
        target_tag="${target_ver#v}"
        target_tag="v${target_tag}"
    fi

    echo "==> Aborting / rolling back release state..."

    # 1. Restore uncommitted changes to release version files
    echo "==> Restoring uncommitted release file changes..."
    git checkout -- Cargo.toml pilot.spec data/io.github.irelandqlan.Pilot.metainfo.xml 2>/dev/null || true

    # 2. Check if current HEAD has an unpushed release bump commit
    local current_branch
    current_branch=$(git rev-parse --abbrev-ref HEAD)
    local commit_msg
    commit_msg=$(git log -1 --pretty=%B 2>/dev/null || true)
    if [[ "$commit_msg" =~ ^chore:\ bump\ version\ to ]]; then
        if git rev-parse --verify "origin/${current_branch}" >/dev/null 2>&1; then
            local local_rev remote_rev
            local_rev=$(git rev-parse HEAD)
            remote_rev=$(git rev-parse "origin/${current_branch}")
            if [ "$local_rev" != "$remote_rev" ] && git merge-base --is-ancestor "origin/${current_branch}" HEAD; then
                echo "==> Resetting unpushed release commit on branch '${current_branch}' to origin/${current_branch}..."
                git reset --hard "origin/${current_branch}"
            fi
        else
            echo "==> Resetting unpushed release commit on branch '${current_branch}'..."
            git reset --hard HEAD~1
        fi
    fi

    # 3. Check if master has an unpushed release bump commit even if currently on develop
    if [ "$current_branch" != "master" ] && git rev-parse --verify master >/dev/null 2>&1; then
        local master_msg
        master_msg=$(git log -1 --pretty=%B master 2>/dev/null || true)
        if [[ "$master_msg" =~ ^chore:\ bump\ version\ to ]]; then
            if git rev-parse --verify origin/master >/dev/null 2>&1; then
                local m_local m_remote
                m_local=$(git rev-parse master)
                m_remote=$(git rev-parse origin/master)
                if [ "$m_local" != "$m_remote" ] && git merge-base --is-ancestor origin/master master; then
                    echo "==> Resetting unpushed release commit on branch 'master' to origin/master..."
                    git branch -f master origin/master
                fi
            fi
        fi
    fi

    # 4. Remove local release tag(s)
    if [ -n "$target_tag" ]; then
        if git rev-parse "$target_tag" >/dev/null 2>&1; then
            echo "==> Deleting local tag $target_tag..."
            git tag -d "$target_tag" || true
        fi
    else
        for tag in $(git tag -l); do
            if git remote | grep -q origin; then
                if ! git ls-remote --tags origin "refs/tags/${tag}" 2>/dev/null | grep -q "refs/tags/${tag}"; then
                    echo "==> Deleting unpushed local tag ${tag}..."
                    git tag -d "${tag}" || true
                fi
            fi
        done
    fi

    # 5. Return to develop branch if currently on master
    if [ "$current_branch" = "master" ] && git rev-parse --verify develop >/dev/null 2>&1; then
        echo "==> Switching back to 'develop' branch..."
        git checkout develop
    fi

    # 6. Clean build artifacts and packaging outputs
    echo "==> Cleaning release build directories..."
    rm -rf build dist .flatpak-builder

    echo ""
    echo "Release aborted and workspace cleaned successfully."
    echo "Current branch: $(git rev-parse --abbrev-ref HEAD)"
    git status --short
}

cmd_create() {
    local raw_ver="$1"
    local version="${raw_ver#v}"
    local tag="v${version}"
    local original_branch
    original_branch=$(git rev-parse --abbrev-ref HEAD)
    local release_success=false

    cleanup_on_error() {
        if [ "$release_success" != "true" ]; then
            echo ""
            echo "Warning: Release process interrupted or failed!"
            echo "To cleanly roll back changes and return to '${original_branch}', run:"
            echo "  just release abort ${version}"
        fi
    }
    trap cleanup_on_error EXIT INT TERM

    echo "==> Preparing release ${tag}..."

    # Pre-flight check: working tree clean
    if ! git diff-index --quiet HEAD --; then
        echo "Error: Working tree has uncommitted changes. Please commit or stash them first."
        exit 1
    fi

    # Pre-flight check: branch check
    if [ "$original_branch" != "develop" ] && [ "$original_branch" != "master" ]; then
        echo "Error: Releases must be triggered from 'develop' or 'master' branch (currently on '$original_branch')."
        exit 1
    fi

    # Pre-flight check: ensure tag does not already exist
    if git rev-parse "${tag}" >/dev/null 2>&1; then
        echo "Error: Tag '${tag}' already exists locally! Run 'just release abort ${version}' if this was an interrupted release."
        exit 1
    fi
    if git remote | grep -q origin; then
        if git ls-remote --tags origin "refs/tags/${tag}" 2>/dev/null | grep -q "refs/tags/${tag}"; then
            echo "Error: Tag '${tag}' already exists on remote origin!"
            exit 1
        fi
    fi

    # If starting on develop, merge into master first
    if [ "$original_branch" = "develop" ]; then
        echo "==> Merging develop into master first..."
        cmd_sync
        git checkout master
    fi

    echo "==> Running test suite..."
    cargo test --quiet

    echo "==> Updating version to ${version} in Cargo.toml..."
    sed -i -E 's/^version = "[^"]*"/version = "'"${version}"'"/' Cargo.toml
    if [ -f "pilot.spec" ]; then
        sed -i -E 's/^Version:\s+[0-9.]+/Version:        '"${version}"'/' pilot.spec
    fi
    cargo check --quiet

    local today
    today=$(date +%Y-%m-%d)
    local metainfo="data/io.github.irelandqlan.Pilot.metainfo.xml"
    if [ -f "$metainfo" ] && grep -q "<releases>" "$metainfo"; then
        if ! grep -q "version=\"${version}\"" "$metainfo"; then
            sed -i -E "s|<releases>|<releases>\n    <release version=\"${version}\" date=\"$today\" />|" "$metainfo"
        fi
    fi

    # Commit version bump if there are changes
    if ! git diff-index --quiet HEAD --; then
        echo "==> Committing release bump..."
        git commit -am "chore: bump version to ${version}"
    else
        echo "==> Version already set to ${version} on master, skipping commit."
    fi

    echo "==> Creating git tag ${tag}..."
    git tag -a "${tag}" -m "Release ${tag}"

    if git remote | grep -q origin; then
        echo "==> Pushing commit and tag to GitHub..."
        git push origin master
        git push origin "${tag}"
    fi

    # If release started from develop, merge master back into develop so versions stay in sync
    if [ "$original_branch" = "develop" ]; then
        echo "==> Updating develop with the release commit..."
        git checkout develop
        git merge master -m "Merge master into develop after ${tag}"
        if git remote | grep -q origin; then
            git push origin develop
        fi
    fi

    release_success=true
    echo ""
    echo "Release ${tag} created and published successfully!"
}

# Main dispatcher
ACTION="${1:-help}"
shift || true

case "$ACTION" in
    help|--help|-h)
        usage
        ;;
    sync)
        cmd_sync
        ;;
    abort)
        cmd_abort "${1:-}"
        ;;
    create)
        if [ -z "${1:-}" ]; then
            echo "Error: Version required. Example: just release create 0.2.1"
            exit 1
        fi
        cmd_create "$1"
        ;;
    v[0-9]*|[0-9]*)
        cmd_create "$ACTION"
        ;;
    *)
        echo "Error: Unknown command or invalid version: $ACTION"
        usage 1
        ;;
esac
