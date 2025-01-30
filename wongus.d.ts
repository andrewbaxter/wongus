declare type Wongus = {
  /**
   * Commandline `k=v` arguments
   */
  readonly args: Map<string, string>;
  /**
   * Environment variables at time of launching wongus
   */
  readonly env: Map<string, string>;
  /**
   * Write a message to the stderr of the wongus process, rather than the console
   */
  readonly log: (message: any) => void;
  /**
   * Run a command and wait for it to exit, returning the stdout and stderr. The process is killed if it takes longer than `timeout_secs`.
   */
  readonly run_command: (args: {
    command: string[];
    working_dir?: string;
    environment?: { [key: string]: string };
    /**
     * Defaults to 10
     */
    timeout_secs?: number;
  }) => Promise<{
    stdout: string;
    stderr: string;
  }>;
  /**
   * Run a command and don't wait for it to exit.
   */
  readonly run_detached_command: (args: {
    command: string[];
    working_dir?: string;
    environment?: { [key: string]: string };
  }) => Promise<{
    pid: number;
  }>;
  /**
   * Run a command and call `cb` with each line it writes to stdout.
   */
  readonly stream_command: (args: {
    command: string[];
    working_dir?: string;
    environment?: { [key: string]: string };
    cb: (line: string) => void;
  }) => void;
  /**
   * Read a file, return the contents as a string
   */
  readonly read: (path: string) => Promise<string>;
  /**
   * Overwrite this with a callback that's called when an external process uses wongus's IPC.
   */
  handle_external_ipc: (body: any) => any;
};
interface Window {
  wongus: Wongus;
}
declare const wongus: Wongus;
