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

# Clean all build artifacts, temporary caches, and packaging outputs
clean:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf build dist target/release target/debug .flatpak-builder
    find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
    echo "✨ Cleaned build directories, packaging outputs, and caches."


# Build and install Flatpak package locally
flatpak:
    flatpak-builder --disable-rofiles-fuse --force-clean --user --install build/flatpak io.github.irelandqlan.Pilot.yml

# Run the installed Flatpak application
flatpak-run:
    flatpak run io.github.irelandqlan.Pilot

# Build standalone Flatpak bundle (.flatpak file in dist/)
flatpak-bundle:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version =' Cargo.toml | head -n1 | cut -d'"' -f2)
    mkdir -p dist build/flatpak-repo
    echo "==> Building Flatpak ostree repository..."
    flatpak-builder --disable-rofiles-fuse --force-clean --repo=build/flatpak-repo build/flatpak io.github.irelandqlan.Pilot.yml
    echo "==> Creating standalone bundle dist/pilot-v${VERSION}.flatpak..."
    flatpak build-bundle build/flatpak-repo "dist/pilot-v${VERSION}.flatpak" io.github.irelandqlan.Pilot master
    echo "✅ Flatpak bundle ready at dist/pilot-v${VERSION}.flatpak"

# Build Fedora RPM package into dist/
rpm:
    #!/usr/bin/env bash
    set -euo pipefail
    VERSION=$(grep '^version =' Cargo.toml | head -n1 | cut -d'"' -f2)
    echo "==> Packaging Pilot v${VERSION} RPM..."
    BUILD_DIR="$(pwd)/build/rpm"
    rm -rf "${BUILD_DIR}"
    mkdir -p "${BUILD_DIR}/"{BUILD,RPMS,SOURCES,SPECS,SRPMS} dist
    TARBALL="${BUILD_DIR}/SOURCES/pilot-${VERSION}.tar.gz"
    tar --exclude-vcs --exclude="./target" --exclude="./venv" --exclude="./build" --exclude="./dist" --exclude="./models" \
        --transform "s,^\.,pilot-${VERSION}," -czf "${TARBALL}" .
    rpmbuild --define "_topdir ${BUILD_DIR}" -ba pilot.spec
    cp "${BUILD_DIR}/RPMS/"*/*.rpm dist/
    echo ""
    echo "✅ Pilot RPM built successfully in dist/:"
    ls -lh dist/*.rpm

# Install built RPM onto the system
rpm-install: rpm
    #!/usr/bin/env bash
    set -euo pipefail
    RPM_FILE=$(ls -t dist/pilot-*.rpm | head -n1)
    echo "==> Installing ${RPM_FILE} via dnf..."
    sudo dnf install -y --nogpgcheck "${RPM_FILE}"
    echo "✅ Pilot installed! Launch with 'pilot' or manage with 'pilot-ctl status'."

# ── System Installation & Service Management ────────────────────────────────

# Inspect running services, Bluetooth connection, and input permissions
status:
    ./bin/pilot-ctl status

# Restart all background daemons
restart:
    ./bin/pilot-ctl restart

# Follow combined journalctl output for Pilot daemons
logs:
    ./bin/pilot-ctl logs

# Stop all background daemons
stop:
    ./bin/pilot-ctl stop

# Start all background daemons
start:
    ./bin/pilot-ctl start

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

    VERSION="{{version}}"
    TAG="v${VERSION#v}"
    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
    RELEASE_SUCCESS=false

    cleanup_on_error() {
        if [ "$RELEASE_SUCCESS" != "true" ]; then
            echo ""
            echo "⚠️ Release process interrupted or failed!"
            echo "   To cleanly roll back changes and return to '${CURRENT_BRANCH}', run:"
            echo "   just abort-release ${VERSION}"
        fi
    }
    trap cleanup_on_error EXIT INT TERM

    echo "==> Preparing release ${TAG}..."

    # Pre-flight check: Ensure working tree is clean
    if ! git diff-index --quiet HEAD --; then
        echo "❌ Error: Working tree has uncommitted changes. Please commit or stash them first."
        exit 1
    fi

    # Pre-flight check: Verify branch
    if [ "$CURRENT_BRANCH" != "develop" ] && [ "$CURRENT_BRANCH" != "master" ]; then
        echo "❌ Error: Releases must be triggered from 'develop' or 'master' branch (currently on '$CURRENT_BRANCH')."
        exit 1
    fi

    # Pre-flight check: Ensure tag does not already exist locally or remotely
    if git rev-parse "${TAG}" >/dev/null 2>&1; then
        echo "❌ Error: Tag '${TAG}' already exists locally! Run 'just abort-release ${VERSION}' if this was an interrupted release."
        exit 1
    fi
    if git remote | grep -q origin; then
        if git ls-remote --tags origin "refs/tags/${TAG}" 2>/dev/null | grep -q "refs/tags/${TAG}"; then
            echo "❌ Error: Tag '${TAG}' already exists on remote origin!"
            exit 1
        fi
    fi

    if [ "$CURRENT_BRANCH" = "develop" ]; then
        echo "==> Merging develop to master first..."
        just merge-to-master
        git checkout master
    fi

    echo "==> Running test suite..."
    cargo test --quiet

    echo "==> Updating version to ${VERSION} in Cargo.toml..."
    sed -i -E 's/^version = "[^"]*"/version = "'"${VERSION}"'"/' Cargo.toml
    if [ -f "pilot.spec" ]; then
        sed -i -E 's/^Version:\s+[0-9.]+/Version:        '"${VERSION}"'/' pilot.spec
    fi
    cargo check --quiet

    TODAY=$(date +%Y-%m-%d)
    METAINFO="data/io.github.irelandqlan.Pilot.metainfo.xml"
    if [ -f "$METAINFO" ] && grep -q "<releases>" "$METAINFO"; then
        if ! grep -q "version=\"${VERSION}\"" "$METAINFO"; then
            sed -i -E "s|<releases>|<releases>\n    <release version=\"${VERSION}\" date=\"$TODAY\" />|" "$METAINFO"
        fi
    fi

    echo "==> Committing release bump..."
    git commit -am "chore: bump version to ${VERSION}"

    echo "==> Creating git tag ${TAG}..."
    git tag -a "${TAG}" -m "Release ${TAG}"

    if git remote | grep -q origin; then
        echo "==> Pushing commit and tag to GitHub..."
        git push origin master
        git push origin "${TAG}"
    fi

    if [ "$CURRENT_BRANCH" = "develop" ]; then
        echo "==> Updating develop with the release commit..."
        git checkout develop
        git merge master --ff-only || git merge master -m "Merge master into develop after ${TAG}"
        if git remote | grep -q origin; then
            git push origin develop
        fi
    fi

    RELEASE_SUCCESS=true
    echo ""
    echo "🎉 Release ${TAG} created and published!"

# Abort or roll back an interrupted or broken release (e.g.: just abort-release [version])
abort-release version="":
    #!/usr/bin/env bash
    set -euo pipefail

    echo "==> Aborting / rolling back release state..."

    TARGET_VER="{{version}}"
    if [ -n "$TARGET_VER" ]; then
        TARGET_TAG="${TARGET_VER#v}"
        TARGET_TAG="v${TARGET_TAG}"
    else
        TARGET_TAG=""
    fi

    # 1. Restore any uncommitted changes to release version files
    echo "==> Restoring uncommitted release file changes..."
    git checkout -- Cargo.toml pilot.spec data/io.github.irelandqlan.Pilot.metainfo.xml 2>/dev/null || true

    # 2. Check if current HEAD has an unpushed release bump commit
    CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
    COMMIT_MSG=$(git log -1 --pretty=%B 2>/dev/null || true)
    if [[ "$COMMIT_MSG" =~ ^chore:\ bump\ version\ to ]]; then
        if git rev-parse --verify "origin/${CURRENT_BRANCH}" >/dev/null 2>&1; then
            LOCAL_REV=$(git rev-parse HEAD)
            REMOTE_REV=$(git rev-parse "origin/${CURRENT_BRANCH}")
            if [ "$LOCAL_REV" != "$REMOTE_REV" ] && git merge-base --is-ancestor "origin/${CURRENT_BRANCH}" HEAD; then
                echo "==> Resetting unpushed release commit on branch '${CURRENT_BRANCH}' to origin/${CURRENT_BRANCH}..."
                git reset --hard "origin/${CURRENT_BRANCH}"
            fi
        else
            echo "==> Resetting unpushed release commit on branch '${CURRENT_BRANCH}'..."
            git reset --hard HEAD~1
        fi
    fi

    # 3. Check if master has an unpushed release bump commit even if currently on another branch
    if [ "$CURRENT_BRANCH" != "master" ] && git rev-parse --verify master >/dev/null 2>&1; then
        MASTER_MSG=$(git log -1 --pretty=%B master 2>/dev/null || true)
        if [[ "$MASTER_MSG" =~ ^chore:\ bump\ version\ to ]]; then
            if git rev-parse --verify origin/master >/dev/null 2>&1; then
                M_LOCAL=$(git rev-parse master)
                M_REMOTE=$(git rev-parse origin/master)
                if [ "$M_LOCAL" != "$M_REMOTE" ] && git merge-base --is-ancestor origin/master master; then
                    echo "==> Resetting unpushed release commit on branch 'master' to origin/master..."
                    git branch -f master origin/master
                fi
            fi
        fi
    fi

    # 4. Remove local release tag(s)
    if [ -n "$TARGET_TAG" ]; then
        if git rev-parse "$TARGET_TAG" >/dev/null 2>&1; then
            echo "==> Deleting local tag $TARGET_TAG..."
            git tag -d "$TARGET_TAG" || true
        fi
    else
        # Auto-detect unpushed local tags
        for TAG in $(git tag -l); do
            if git remote | grep -q origin; then
                if ! git ls-remote --tags origin "refs/tags/${TAG}" 2>/dev/null | grep -q "refs/tags/${TAG}"; then
                    echo "==> Deleting unpushed local tag ${TAG}..."
                    git tag -d "${TAG}" || true
                fi
            fi
        done
    fi

    # 5. Return to develop branch if currently on master
    if [ "$CURRENT_BRANCH" = "master" ] && git rev-parse --verify develop >/dev/null 2>&1; then
        echo "==> Switching back to 'develop' branch..."
        git checkout develop
    fi

    # 6. Clean build artifacts and packaging outputs
    echo "==> Cleaning release build directories (build/, dist/)..."
    rm -rf build dist .flatpak-builder

    echo ""
    echo "✅ Release aborted and workspace cleaned successfully!"
    echo "   Current branch: $(git rev-parse --abbrev-ref HEAD)"
    git status --short

# Alias for abort-release
release-abort version="":
    @just abort-release {{version}}

