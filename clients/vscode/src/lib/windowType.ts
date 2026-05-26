import { isDescribeFile, isDiffFile, isLogFile, type UriLike } from "./uri";

export function windowTypeForUri(uri: UriLike | undefined): string {
  if (!uri) return "status";
  if (isLogFile(uri)) return "log";
  if (isDiffFile(uri)) return "diff";
  if (isDescribeFile(uri)) return "describe";
  return "status";
}
