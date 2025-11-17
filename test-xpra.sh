#!/bin/bash
# Xpra integration smoke tests
#
# This helper mirrors the ignored tests in tests/e2e_testing_guide.rs but runs
# a minimal happy path automatically when possible.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

VM_NAME="xpra-test-$(date +%s)"
ACTION="${1:-all}"

log()  { echo -e "${BLUE}ℹ${NC} $*"; }
ok()   { echo -e "${GREEN}✓${NC} $*"; }
warn() { echo -e "${YELLOW}⚠${NC} $*"; }
err()  { echo -e "${RED}✗${NC} $*"; }

check_host_tools() {
    local missing=0
    for tool in xpra waypipe virsh ssh vm-provisioner; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            err "Missing $tool"
            missing=1
        else
            ok "$tool present"
        fi
    done
    if [ "$missing" -ne 0 ]; then
        err "Install the tools above before continuing"
        exit 1
    fi
    if command -v mullvad >/dev/null 2>&1; then
        log "Ensure Mullvad allows LAN traffic: mullvad lan set allow on"
    fi
}

create_vm() {
    log "Creating XPRA VM $VM_NAME"
    ./target/release/vm-provisioner create \
        --display-protocol xpra \
        --system firefox \
        --memory 4096 \
        --name "$VM_NAME" \
        --yes
    ok "VM created"
}

start_and_launch() {
    log "Starting VM"
    ./target/release/vm-provisioner start "$VM_NAME"
    log "Generating shortcuts"
    ./target/release/vm-provisioner generate-shortcuts "$VM_NAME"
    log "Launching firefox via xpra"
    ./target/release/vm-provisioner launch "$VM_NAME" firefox &
    XPRA_PID=$!
    sleep 5
    if pgrep -f "xpra" >/dev/null 2>&1; then
        ok "xpra client running"
        warn "Close the xpra window to continue"
        wait "$XPRA_PID"
    else
        warn "xpra process not detected (maybe already closed?)"
    fi
}

cleanup() {
    log "Destroying VM"
    ./target/release/vm-provisioner destroy "$VM_NAME" -y || true
}

case "$ACTION" in
    host)
        check_host_tools
        ;;
    create)
        check_host_tools
        create_vm
        ;;
    launch)
        start_and_launch
        ;;
    clean)
        cleanup
        ;;
    all)
        check_host_tools
        create_vm
        start_and_launch
        cleanup
        ;;
    *)
        err "Unknown action $ACTION"
        echo "Usage: $0 [host|create|launch|clean|all]"
        exit 1
        ;;
esac
