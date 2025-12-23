"""
Glass Box UI Entry Point

Run the FastHTML web interface for Casparian Flow.
"""
import argparse
import uvicorn


def main():
    parser = argparse.ArgumentParser(description="Run Casparian Flow Glass Box UI")
    parser.add_argument(
        "--port", 
        type=int, 
        default=5000, 
        help="Port to run the UI server on (default: 5000)"
    )
    parser.add_argument(
        "--host",
        type=str,
        default="127.0.0.1",
        help="Host to bind to (default: 127.0.0.1)"
    )
    parser.add_argument(
        "--reload",
        action="store_true",
        help="Enable auto-reload for development"
    )
    
    args = parser.parse_args()

    print(f"Starting Casparian Flow Glass Box UI")
    print(f"   URL: http://{args.host}:{args.port}")
    print(f"   Auto-reload: {'enabled' if args.reload else 'disabled'}")
    
    uvicorn.run(
        "casparian_flow.ui.app:serve",
        host=args.host,
        port=args.port,
        reload=args.reload,
    )


if __name__ == "__main__":
    main()
