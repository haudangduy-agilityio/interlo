import { Component, type ErrorInfo, type ReactNode } from "react";
import { useT } from "../shared/i18n";
import { Button, ErrorState } from "../shared/ui";

function Fallback({ onReload }: { onReload: () => void }) {
  const t = useT();
  return (
    <div className="stack">
      <ErrorState message={t("error.crashed")} />
      <div>
        <Button variant="primary" onClick={onReload}>
          {t("common.reload")}
        </Button>
      </div>
    </div>
  );
}

interface Props {
  children: ReactNode;
  /** Runs when the owner chooses Reload. The default reloads the window. */
  onReload?: () => void;
}

/**
 * The last line of defence: an error while rendering shows the failure and a Reload button
 * instead of a blank window. It must be a class, because only a class can catch render errors.
 */
export class ErrorBoundary extends Component<Props, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // The error name and the component stack only: the message can hold transcript text.
    console.error("The screen stopped:", error.name, info.componentStack);
  }

  render(): ReactNode {
    if (this.state.failed) {
      return <Fallback onReload={this.props.onReload ?? (() => window.location.reload())} />;
    }
    return this.props.children;
  }
}
