"""Entry point for the packaged local service, not a developer interpreter."""
import multiprocessing

if __name__ == "__main__":
    multiprocessing.freeze_support()
    from quantix.__main__ import main
    main()
