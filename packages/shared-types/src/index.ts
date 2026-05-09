export const IPC_PROTOCOL_VERSION = 1;

export type BrowserBridgeRequest =
  | { type: "GET_STATUS" }
  | { type: "WATCH_STATUS"; interval_ms: number }
  | { type: "START_SESSION"; relying_party: string }
  | { type: "CANCEL_SESSION"; session_id: string };

export type RustClientPayload =
  | { type: "GET_STATUS" }
  | { type: "WATCH_STATUS"; data: { interval_ms: number } }
  | { type: "START_SESSION"; data: { relying_party: string } }
  | { type: "CANCEL_SESSION"; data: { session_id: string } };

export type RustDaemonPayload =
  | {
      type: "STATUS";
      data: {
        healthy: boolean;
        pcsc_available: boolean;
        active_session_count: number;
        readers: { name: string; card_present: boolean; error?: string | null }[];
        diagnostics: string[];
        last_error?: string | null;
      };
    }
  | { type: "SESSION_STARTED"; data: { session_id: string } }
  | { type: "SESSION_CANCELLED"; data: { session_id: string } }
  | { type: "ERROR"; data: { code: string; message: string } };

export type IpcEnvelope<TPayload> = {
  protocol_version: number;
  request_id: string;
  payload: TPayload;
};
