import { Component, type ErrorInfo, type ReactNode } from "react";

interface AppErrorBoundaryProps {
  children: ReactNode;
}

interface AppErrorBoundaryState {
  error: Error | null;
}

export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("CGswitch 界面渲染失败", error, info.componentStack);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <main className="flex h-screen items-center justify-center bg-[var(--app-bg)] p-6">
        <section className="apple-group w-full max-w-lg text-center">
          <h1 className="apple-title">界面加载失败</h1>
          <p className="muted mt-2 text-sm">当前页面遇到异常，可以重新加载后继续使用。</p>
          <button type="button" className="apple-action-button app-button--primary mt-5" onClick={() => window.location.reload()}>
            重新加载
          </button>
          <details className="muted mt-4 text-left text-xs">
            <summary className="cursor-pointer">查看错误详情</summary>
            <pre className="mt-2 whitespace-pre-wrap break-words">{this.state.error.message}</pre>
          </details>
        </section>
      </main>
    );
  }
}
