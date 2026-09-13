import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportReactBoundary } from "../diagnostics";

type Props = { children: ReactNode };
type State = { failed: boolean };

export class AppErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: unknown, _info: ErrorInfo) {
    reportReactBoundary(error);
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <main className="app-error-boundary" role="alert">
        <div className="app-error-boundary-card">
          <h1>Quantix could not display this page</h1>
          <p>Try again. Your saved Tender files are still on this device.</p>
          <button
            type="button"
            className="button primary"
            onClick={() => this.setState({ failed: false })}
          >
            Try again
          </button>
        </div>
      </main>
    );
  }
}
