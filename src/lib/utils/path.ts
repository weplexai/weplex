// Cross-platform path shortening utilities

/** Replace home directory prefix with ~ (works on macOS, Linux, and Windows) */
export function tildeHome(path: string): string {
  // Unix: /Users/alice/... or /home/alice/...
  const unix = path.replace(/^\/(?:Users|home)\/[^/]+/, '~');
  if (unix !== path) return unix;
  // Windows: C:\Users\alice\...
  return path.replace(/^[a-zA-Z]:\\Users\\[^\\]+/, '~');
}

/** Shorten path for display: ~/very/deep/nested/path → …/nested/path */
export function shortPath(path: string): string {
  const withTilde = tildeHome(path);
  const parts = withTilde.split('/').filter(Boolean);
  if (parts.length <= 2) return withTilde;
  return '…/' + parts.slice(-2).join('/');
}
