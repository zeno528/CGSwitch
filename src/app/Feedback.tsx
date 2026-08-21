import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Toast from "@radix-ui/react-toast";
import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";

type ToastTone = "success" | "error" | "warning" | "info";

interface ConfirmOptions {
  title: string;
  description: ReactNode;
  confirmText?: string;
  cancelText?: string;
  destructive?: boolean;
}

interface FeedbackContextValue {
  showToast: (tone: ToastTone, message: string) => void;
  success: (message: string) => void;
  error: (message: string) => void;
  warning: (message: string) => void;
  info: (message: string) => void;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
}

const FeedbackContext = createContext<FeedbackContextValue | null>(null);

interface ToastState {
  id: number;
  tone: ToastTone;
  message: string;
}

interface ConfirmationState extends ConfirmOptions {
  resolve: (confirmed: boolean) => void;
}

export function FeedbackProvider({ children }: { children: ReactNode }) {
  const [toast, setToast] = useState<ToastState | null>(null);
  const [confirmation, setConfirmation] = useState<ConfirmationState | null>(null);
  const confirmActionRef = useRef<HTMLButtonElement>(null);

  const showToast = useCallback((tone: ToastTone, message: string) => {
    setToast({ id: Date.now(), tone, message });
  }, []);

  const confirm = useCallback((options: ConfirmOptions) => {
    return new Promise<boolean>((resolve) => setConfirmation({ ...options, resolve }));
  }, []);

  const closeConfirmation = useCallback((confirmed: boolean) => {
    setConfirmation((current) => {
      current?.resolve(confirmed);
      return null;
    });
  }, []);

  const value: FeedbackContextValue = {
    showToast,
    success: (message) => showToast("success", message),
    error: (message) => showToast("error", message),
    warning: (message) => showToast("warning", message),
    info: (message) => showToast("info", message),
    confirm,
  };

  return (
    <FeedbackContext.Provider value={value}>
      <Toast.Provider swipeDirection="right">
        {children}
        <Toast.Root
          key={toast?.id}
          open={toast !== null}
          duration={4200}
          onOpenChange={(open) => {
            if (!open) setToast(null);
          }}
          className={`app-toast app-toast--${toast?.tone ?? "info"}`}
        >
          <Toast.Description>{toast?.message}</Toast.Description>
        </Toast.Root>
        <Toast.Viewport className="app-toast-viewport" />
      </Toast.Provider>

      <AlertDialog.Root
        open={confirmation !== null}
        onOpenChange={(open) => {
          if (!open) closeConfirmation(false);
        }}
      >
        <AlertDialog.Portal>
          <AlertDialog.Overlay className="app-dialog-overlay" />
          <AlertDialog.Content
            className="app-dialog-content app-dialog-content--confirm"
            onOpenAutoFocus={(event) => {
              if (!confirmActionRef.current) return;
              event.preventDefault();
              confirmActionRef.current.focus({ preventScroll: true });
            }}
          >
            <AlertDialog.Title className="app-dialog-title">{confirmation?.title}</AlertDialog.Title>
            <AlertDialog.Description className="app-dialog-description">
              {confirmation?.description}
            </AlertDialog.Description>
            <div className="app-dialog-actions">
              <AlertDialog.Cancel className="apple-action-button" onClick={() => closeConfirmation(false)}>
                {confirmation?.cancelText ?? "取消"}
              </AlertDialog.Cancel>
              <AlertDialog.Action
                ref={confirmActionRef}
                className={`apple-action-button ${confirmation?.destructive ? "app-button--danger" : "app-button--primary"}`}
                onClick={() => closeConfirmation(true)}
              >
                {confirmation?.confirmText ?? "确定"}
              </AlertDialog.Action>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </FeedbackContext.Provider>
  );
}

export function useFeedback(): FeedbackContextValue {
  const value = useContext(FeedbackContext);
  if (!value) throw new Error("useFeedback must be used inside FeedbackProvider");
  return value;
}
