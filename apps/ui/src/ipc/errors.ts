import { t } from "../shared/i18n";
import { IPC_ERROR_CODES, type IpcErrorCode } from "./types";

/** An IpcError, or a failure the contract does not name (shell crash, bad payload). */
export interface UiError {
  code: IpcErrorCode | "UNKNOWN";
  message: string;
  /** The failed check ids of START_CHECK_FAILED. */
  details?: string[];
}

const CODES: readonly string[] = IPC_ERROR_CODES;

function isIpcError(
  value: unknown,
): value is { code: IpcErrorCode; message: string; details?: unknown } {
  if (typeof value !== "object" || value === null) return false;
  const v = value as { code?: unknown; message?: unknown };
  return typeof v.code === "string" && CODES.includes(v.code) && typeof v.message === "string";
}

function isStringList(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isUiError(value: unknown): value is UiError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as { code?: unknown; message?: unknown };
  return v.code === "UNKNOWN" && typeof v.message === "string";
}

/** Normalize anything a rejected promise can carry. Safe to apply twice: a UiError keeps its message. */
export function toUiError(value: unknown): UiError {
  if (isIpcError(value)) {
    const { code, message, details } = value;
    // Only START_CHECK_FAILED carries details. Any other shape is dropped.
    if (code === "START_CHECK_FAILED" && isStringList(details)) return { code, message, details };
    return { code, message };
  }
  if (isUiError(value)) return { code: "UNKNOWN", message: value.message };
  if (typeof value === "string" && value.length > 0) return { code: "UNKNOWN", message: value };
  if (value instanceof Error) return { code: "UNKNOWN", message: value.message };
  return { code: "UNKNOWN", message: t("error.unknown") };
}
