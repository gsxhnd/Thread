import { Component, type ErrorInfo, type ReactNode } from "react";
import i18n from "@/i18n";
import { AppFatalScreen } from "@/components/app-fatal-screen";

type AppErrorBoundaryProps = {
  children: ReactNode;
};

type AppErrorBoundaryState = {
  error: Error | null;
  resetKey: number;
};

/** Catches render errors and offers retry / reload instead of a blank window. */
export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = {
    error: null,
    resetKey: 0,
  };

  static getDerivedStateFromError(error: Error): Partial<AppErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("App render error:", error, info.componentStack);
  }

  private handleRetry = () => {
    this.setState((state) => ({
      error: null,
      resetKey: state.resetKey + 1,
    }));
  };

  render() {
    const { error, resetKey } = this.state;

    if (error) {
      return (
        <AppFatalScreen
          title={i18n.t("common:errorBoundary.title")}
          description={i18n.t("common:errorBoundary.description")}
          detail={error.message}
          onRetry={this.handleRetry}
        />
      );
    }

    return <div key={resetKey} className="contents">{this.props.children}</div>;
  }
}
