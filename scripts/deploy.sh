#!/bin/bash
# deploy.sh - One-command deployment script for HotStuff Blockchain

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
CONTAINER_NAME="atomiq-blockchain"
IMAGE_NAME="atomiq-blockchain:latest"
API_PORT="8080"
METRICS_PORT="9090"
COMPOSE_CMD=""  # Will be set in check_prerequisites

# Functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

check_prerequisites() {
    log_info "Checking prerequisites..."
    
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed. Please install Docker first."
        exit 1
    fi
    
    # Check for Docker Compose (v2 or v1)
    if command -v "docker-compose" &> /dev/null; then
        COMPOSE_CMD="docker-compose"
    elif docker compose version &> /dev/null; then
        COMPOSE_CMD="docker compose"
    else
        log_error "Docker Compose is not installed. Please install Docker Compose first."
        exit 1
    fi
    
    log_success "Prerequisites check passed (using $COMPOSE_CMD)"
}

cleanup_existing() {
    log_info "Cleaning up existing containers..."
    
    # Stop and remove existing containers
    $COMPOSE_CMD down 2>/dev/null || true
    docker container rm -f $CONTAINER_NAME 2>/dev/null || true
    
    log_success "Cleanup completed"
}

build_and_deploy() {
    log_info "Building blockchain image..."
    $COMPOSE_CMD build --no-cache blockchain
    
    log_info "Starting blockchain services..."
    $COMPOSE_CMD up -d blockchain
    
    log_success "Services started"
}

wait_for_health() {
    log_info "Waiting for blockchain to become healthy..."
    
    local retries=30
    local count=0
    
    while [ $count -lt $retries ]; do
        if curl -f -s http://localhost:$API_PORT/health > /dev/null 2>&1; then
            log_success "Blockchain is healthy!"
            return 0
        fi
        
        ((count++))
        echo -n "."
        sleep 2
    done
    
    log_error "Blockchain failed to become healthy within 60 seconds"
    $COMPOSE_CMD logs blockchain
    exit 1
}

show_status() {
    log_info "Deployment successful! 🚀"
    echo
    echo "📊 Blockchain Status:"
    curl -s http://localhost:$API_PORT/status | jq . 2>/dev/null || echo "Status endpoint not ready yet"
    echo
    echo "🔗 Available endpoints:"
    echo "   Health:      http://localhost:$API_PORT/health"
    echo "   Status:      http://localhost:$API_PORT/status" 
    echo "   Blocks:      http://localhost:$API_PORT/blocks"
    echo "   Metrics:     http://localhost:$METRICS_PORT/metrics"
    echo
    echo "📋 Quick commands:"
    echo "   View logs:   $COMPOSE_CMD logs -f blockchain"
    echo "   Stop:        $COMPOSE_CMD down"
    echo "   Restart:     $COMPOSE_CMD restart blockchain"
    echo "   Update:      ./deploy.sh"
    echo
}

perform_quick_test() {
    log_info "Performing quick functionality test..."
    
    # Test health endpoint
    if curl -f -s http://localhost:$API_PORT/health | jq . > /dev/null 2>&1; then
        log_success "Health endpoint working"
    else
        log_warning "Health endpoint test failed"
    fi
    
    # Test status endpoint
    if curl -f -s http://localhost:$API_PORT/status | jq . > /dev/null 2>&1; then
        log_success "Status endpoint working"
    else
        log_warning "Status endpoint test failed"
    fi
    
    # Test blocks endpoint
    if curl -f -s "http://localhost:$API_PORT/blocks?limit=1" | jq . > /dev/null 2>&1; then
        log_success "Blocks endpoint working"
    else
        log_warning "Blocks endpoint test failed"
    fi
}

# Main deployment flow
main() {
    echo "🚀 HotStuff Blockchain Deployment Script"
    echo "========================================"
    
    check_prerequisites
    cleanup_existing
    build_and_deploy
    wait_for_health
    perform_quick_test
    show_status
    
    if [ "$1" = "--logs" ]; then
        echo "📜 Showing live logs (Ctrl+C to exit):"
        $COMPOSE_CMD logs -f blockchain
    fi
}

# Handle script arguments
case "${1:-}" in
    "stop")
        log_info "Stopping blockchain services..."
        $COMPOSE_CMD down
        log_success "Services stopped"
        ;;
    "restart")
        log_info "Restarting blockchain services..."
        $COMPOSE_CMD restart blockchain
        wait_for_health
        log_success "Services restarted"
        ;;
    "logs")
        $COMPOSE_CMD logs -f blockchain
        ;;
    "status")
        curl -s http://localhost:$API_PORT/status | jq .
        ;;
    "health")
        curl -s http://localhost:$API_PORT/health | jq .
        ;;
    "update")
        main
        ;;
    *)
        main "$@"
        ;;
esac