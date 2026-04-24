// Structured logger with level filtering.
// In production, only warn and error are shown.
// Set localStorage 'weplex_debug' = '1' to enable info/debug.

const isDev = import.meta.env.DEV;
const isVerbose = () => isDev || localStorage.getItem('weplex_debug') === '1';

const PREFIX = '[Weplex]';

export const logger = {
  debug(msg: string, ...args: unknown[]) {
    if (isVerbose()) console.debug(PREFIX, msg, ...args);
  },
  info(msg: string, ...args: unknown[]) {
    if (isVerbose()) console.log(PREFIX, msg, ...args);
  },
  warn(msg: string, ...args: unknown[]) {
    console.warn(PREFIX, msg, ...args);
  },
  error(msg: string, ...args: unknown[]) {
    console.error(PREFIX, msg, ...args);
  },
};
