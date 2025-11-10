#!/bin/bash
# X2Go Integration Testing Suite
# This script automates the X2Go testing plan from X2GO_SUPPORT_PLAN.md
#
# USAGE: ./test-x2go.sh [test_number]
# Examples:
#   ./test-x2go.sh 1    # Run only Test 1 (VM Creation)
#   ./test-x2go.sh      # Run all tests sequentially
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
VM_NAME="x2go-test-$(date +%s)"
TEST_VM="${1:-all}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}X2Go Integration Testing Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Test VM Name: $VM_NAME"
echo "Test Plan: See X2GO_SUPPORT_PLAN.md for details"
echo ""

# Helper functions
run_test() {
    local test_num=$1
    local test_name=$2
    echo -e "${BLUE}═══════════════════════════════════════${NC}"
    echo -e "${BLUE}Test $test_num: $test_name${NC}"
    echo -e "${BLUE}═══════════════════════════════════════${NC}"
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

error() {
    echo -e "${RED}✗ $1${NC}"
}

warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

# ============================================================================
# TEST 1: Basic VM Creation
# ============================================================================
test_1_vm_creation() {
    run_test 1 "VM Creation with X2Go Protocol"

    info "Creating X2Go VM with system package (firefox)..."
    info "This will take 15-20 minutes. Please wait..."
    info ""

    # Check if x2goclient is installed first
    if ! command -v x2goclient &> /dev/null; then
        error "x2goclient not found! Cannot proceed with testing."
        echo ""
        echo "Install x2goclient:"
        echo "  Fedora/RHEL: sudo dnf install x2goclient"
        echo "  Debian/Ubuntu: sudo apt install x2goclient"
        echo "  Arch: sudo pacman -S x2goclient"
        return 1
    fi

    success "x2goclient found: $(x2goclient --version 2>/dev/null || echo 'installed')"

    # Create the VM
    if ./target/release/vm-provisioner create \
        --display-protocol x2go \
        --system firefox \
        --name "$VM_NAME" \
        --memory 4096; then
        success "X2Go VM created successfully: $VM_NAME"

        # Verify config file
        config_file="$HOME/.config/vm-provisioner/$VM_NAME.toml"
        if [ -f "$config_file" ]; then
            success "Config file created: $config_file"

            # Check display protocol is set to X2Go
            if grep -q 'display_protocol = "X2Go"' "$config_file"; then
                success "Display protocol correctly set to X2Go"
            else
                error "Display protocol not set to X2Go in config"
                return 1
            fi
        else
            error "Config file not found: $config_file"
            return 1
        fi

        # Wait for VM to fully boot
        info "Waiting for VM to boot (30 seconds)..."
        sleep 30

        # Check if VM is running
        if virsh list --name | grep -q "^$VM_NAME\$"; then
            success "VM is running"
        else
            error "VM failed to start"
            return 1
        fi

        return 0
    else
        error "Failed to create X2Go VM"
        return 1
    fi
}

# ============================================================================
# TEST 2: SSH Connection (Passwordless)
# ============================================================================
test_2_ssh_connection() {
    run_test 2 "SSH Passwordless Connection"

    info "Getting VM IP address..."
    VM_IP=$(virsh domifaddr "$VM_NAME" 2>/dev/null | grep -o "[0-9]\+\.[0-9]\+\.[0-9]\+\.[0-9]\+" | head -1)

    if [ -z "$VM_IP" ]; then
        warning "Could not determine VM IP address. VM may still be booting..."
        info "Waiting additional 30 seconds..."
        sleep 30
        VM_IP=$(virsh domifaddr "$VM_NAME" 2>/dev/null | grep -o "[0-9]\+\.[0-9]\+\.[0-9]\+\.[0-9]\+" | head -1)
    fi

    if [ -z "$VM_IP" ]; then
        error "Failed to get VM IP address"
        return 1
    fi

    success "VM IP: $VM_IP"

    # Test SSH connection
    info "Testing SSH passwordless connection..."
    if ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no user@"$VM_IP" "echo 'SSH test successful'" 2>/dev/null; then
        success "SSH passwordless connection works"

        # Check DISPLAY is set
        info "Checking DISPLAY variable in VM..."
        DISPLAY_VAR=$(ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no user@"$VM_IP" "echo \$DISPLAY" 2>/dev/null)
        if [ -n "$DISPLAY_VAR" ] && [ "$DISPLAY_VAR" != ":0" ] || [ "$DISPLAY_VAR" = ":0" ]; then
            success "DISPLAY is set in VM: $DISPLAY_VAR"
        else
            warning "DISPLAY may not be set correctly: $DISPLAY_VAR"
        fi

        # Check X11/i3 is running
        info "Checking if i3 window manager is running..."
        if ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no user@"$VM_IP" "pgrep -x i3" 2>/dev/null > /dev/null; then
            success "i3 window manager is running"
        else
            warning "i3 may not be running yet. This is normal if VM just booted."
        fi

        return 0
    else
        error "SSH connection failed. VM may still be booting."
        return 1
    fi
}

# ============================================================================
# TEST 3: Application Launching
# ============================================================================
test_3_application_launch() {
    run_test 3 "Application Launching via X2Go"

    info "Launching Firefox via X2Go..."
    info "This will open x2goclient. Close it manually when ready for next test."

    # Try to get fresh VM IP in case it changed
    VM_IP=$(virsh domifaddr "$VM_NAME" 2>/dev/null | grep -o "[0-9]\+\.[0-9]\+\.[0-9]\+\.[0-9]\+" | head -1)

    if [ -z "$VM_IP" ]; then
        error "Cannot get VM IP"
        return 1
    fi

    # Launch the application
    if ./target/release/vm-provisioner launch "$VM_NAME" "firefox"; then
        success "Application launch command executed"
        info "x2goclient should be starting. Please wait for window to appear..."
        sleep 2

        # Check if x2goclient process started
        if pgrep -x x2goclient > /dev/null; then
            success "x2goclient process is running"

            read -p "$(echo -e ${YELLOW}'Did Firefox window appear? (y/n) '${NC})" -n 1 -r
            echo
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                success "Application launched successfully"
                return 0
            else
                error "Application did not appear"
                return 1
            fi
        else
            warning "x2goclient process not found. It may have already closed."
            return 1
        fi
    else
        error "Failed to launch application"
        return 1
    fi
}

# ============================================================================
# TEST 4: Desktop Shortcuts Generation
# ============================================================================
test_4_desktop_shortcuts() {
    run_test 4 "Desktop Shortcuts Generation"

    info "Generating desktop shortcuts for $VM_NAME..."

    if ./target/release/vm-provisioner generate-shortcuts "$VM_NAME"; then
        success "Desktop shortcuts generated successfully"

        # Check if desktop files were created
        desktop_dir="$HOME/.local/share/applications/vm-provisioner"
        if [ -d "$desktop_dir" ]; then
            desktop_files=$(ls -1 "$desktop_dir/${VM_NAME}-"*.desktop 2>/dev/null | wc -l)
            if [ "$desktop_files" -gt 0 ]; then
                success "Found $desktop_files .desktop files"

                # Show first desktop file
                first_file=$(ls -1 "$desktop_dir/${VM_NAME}-"*.desktop 2>/dev/null | head -1)
                if [ -f "$first_file" ]; then
                    info "Sample .desktop file: $(basename $first_file)"
                    info "Content preview:"
                    head -5 "$first_file" | sed 's/^/  /'
                fi

                return 0
            else
                error "No desktop files found in $desktop_dir"
                return 1
            fi
        else
            error "Desktop directory not found: $desktop_dir"
            return 1
        fi
    else
        error "Failed to generate shortcuts"
        return 1
    fi
}

# ============================================================================
# TEST 5: Clipboard Bidirectional Sharing
# ============================================================================
test_5_clipboard() {
    run_test 5 "Clipboard Bidirectional Sharing"

    VM_IP=$(virsh domifaddr "$VM_NAME" 2>/dev/null | grep -o "[0-9]\+\.[0-9]\+\.[0-9]\+\.[0-9]\+" | head -1)

    if [ -z "$VM_IP" ]; then
        error "Cannot get VM IP"
        return 1
    fi

    info "Testing clipboard: VM → Host"

    # Test copying in VM and pasting on host
    test_string="test-from-vm-$(date +%s)"
    if ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no user@"$VM_IP" \
        "echo '$test_string' | xclip -selection clipboard" 2>/dev/null; then

        sleep 1

        # Try to paste on host
        if command -v xclip &> /dev/null; then
            pasted=$(xclip -selection clipboard -o 2>/dev/null)
            if [ "$pasted" = "$test_string" ]; then
                success "Clipboard VM→Host works: received '$test_string'"
            else
                warning "Clipboard content mismatch. Got: '$pasted'"
            fi
        else
            warning "xclip not installed on host, cannot verify clipboard"
        fi
    else
        error "Failed to test clipboard in VM"
        return 1
    fi

    info "Testing clipboard: Host → VM"
    test_string2="test-from-host-$(date +%s)"

    if command -v xclip &> /dev/null; then
        echo -n "$test_string2" | xclip -selection clipboard
        sleep 1

        if ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no user@"$VM_IP" \
            "xclip -selection clipboard -o" 2>/dev/null | grep -q "$test_string2"; then
            success "Clipboard Host→VM works: sent '$test_string2'"
            return 0
        else
            warning "Clipboard Host→VM may not be working correctly"
            return 1
        fi
    else
        warning "xclip not installed on host, skipping Host→VM test"
        return 0
    fi
}

# ============================================================================
# TEST 6: Audio Streaming
# ============================================================================
test_6_audio_streaming() {
    run_test 6 "Audio Streaming via PulseAudio"

    info "This test requires manual verification"
    info "1. Launch Firefox via X2Go: ./target/release/vm-provisioner launch $VM_NAME 'firefox'"
    info "2. Navigate to YouTube: https://youtube.com"
    info "3. Play a video with audio"
    info "4. Check if audio plays on host speakers"

    read -p "$(echo -e ${YELLOW}'Did audio play correctly? (y/n) '${NC})" -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        success "Audio streaming works"
        return 0
    else
        error "Audio not working"
        return 1
    fi
}

# ============================================================================
# TEST 7: VM Lifecycle
# ============================================================================
test_7_vm_lifecycle() {
    run_test 7 "VM Lifecycle Management"

    info "Testing VM stop..."
    if ./target/release/vm-provisioner stop "$VM_NAME"; then
        success "VM stopped successfully"
    else
        error "Failed to stop VM"
        return 1
    fi

    sleep 5

    info "Testing VM start..."
    if ./target/release/vm-provisioner start "$VM_NAME"; then
        success "VM started successfully"
    else
        error "Failed to start VM"
        return 1
    fi

    sleep 10

    info "Testing VM destroy..."
    if ./target/release/vm-provisioner destroy "$VM_NAME" -y; then
        success "VM destroyed successfully"

        # Verify cleanup
        info "Verifying desktop file cleanup..."
        desktop_dir="$HOME/.local/share/applications/vm-provisioner"
        if [ -d "$desktop_dir" ]; then
            remaining=$(ls -1 "$desktop_dir/${VM_NAME}-"*.desktop 2>/dev/null | wc -l)
            if [ "$remaining" -eq 0 ]; then
                success "All desktop files cleaned up"
                return 0
            else
                error "Desktop files not cleaned up: $remaining remaining"
                return 1
            fi
        else
            success "Desktop directory cleaned up"
            return 0
        fi
    else
        error "Failed to destroy VM"
        return 1
    fi
}

# ============================================================================
# Main Test Runner
# ============================================================================
main() {
    local failed_tests=0
    local passed_tests=0

    # Check if vm-provisioner is built
    if [ ! -f ./target/release/vm-provisioner ]; then
        error "vm-provisioner binary not found. Building..."
        cargo build --release
    fi

    # Run tests
    case "$TEST_VM" in
        1)
            if test_1_vm_creation; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        2)
            if test_2_ssh_connection; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        3)
            if test_3_application_launch; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        4)
            if test_4_desktop_shortcuts; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        5)
            if test_5_clipboard; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        6)
            if test_6_audio_streaming; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        7)
            if test_7_vm_lifecycle; then
                ((passed_tests++))
            else
                ((failed_tests++))
            fi
            ;;
        all)
            info "Running all tests sequentially..."

            if test_1_vm_creation; then ((passed_tests++)); else ((failed_tests++)); fi
            echo ""

            if test_2_ssh_connection; then ((passed_tests++)); else ((failed_tests++)); fi
            echo ""

            if test_3_application_launch; then ((passed_tests++)); else ((failed_tests++)); fi
            echo ""

            if test_4_desktop_shortcuts; then ((passed_tests++)); else ((failed_tests++)); fi
            echo ""

            if test_5_clipboard; then ((passed_tests++)); else ((failed_tests++)); fi
            echo ""

            if test_6_audio_streaming; then ((passed_tests++)); else ((failed_tests++)); fi
            echo ""

            if test_7_vm_lifecycle; then ((passed_tests++)); else ((failed_tests++)); fi
            ;;
        *)
            error "Unknown test: $TEST_VM"
            echo "Usage: $0 [test_number|all]"
            echo "  test_number: 1-7 (run specific test)"
            echo "  all: run all tests"
            exit 1
            ;;
    esac

    # Print summary
    echo ""
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}Test Summary${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo -e "Passed: ${GREEN}$passed_tests${NC}"
    echo -e "Failed: ${RED}$failed_tests${NC}"
    echo ""

    if [ "$failed_tests" -eq 0 ]; then
        success "All tests passed!"
        exit 0
    else
        error "Some tests failed"
        exit 1
    fi
}

main
