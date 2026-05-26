export const READONLY_SCHEME = "badjuju-status";
export const DIFF_SCHEME = "badjuju-diff";

export interface UriLike {
  readonly path: string;
  readonly scheme: string;
}

export function isStatusFile(uri: UriLike): boolean {
  return uri.path.endsWith("/status.jujutsu");
}

export function isLogFile(uri: UriLike): boolean {
  return uri.path.endsWith("/log.jujutsu");
}

export function isDiffFile(uri: UriLike): boolean {
  return (
    uri.scheme === DIFF_SCHEME ||
    uri.path.endsWith("/diff.jujutsu") ||
    /\/diff-(change|commit)-[^/]+\.jujutsu$/.test(uri.path)
  );
}

export function isDescribeFile(uri: UriLike): boolean {
  return uri.path.endsWith("/describe.jujutsu");
}

export function isReadonlyOutput(uri: UriLike): boolean {
  return isStatusFile(uri) || isLogFile(uri) || isDiffFile(uri);
}
