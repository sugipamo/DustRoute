"""Shared private instrumented-server lifecycle for current live observations."""
import os
import json
import queue
import re
import subprocess
import threading
import time


class InstrumentedServer:
    def __init__(self, instrumentation, raw_output, console_output, capture_ticks=160, trace_bounds=None, capture_mode="bounded"):
        assert capture_mode in ("bounded", "continuous")
        self.console = console_output.open("x")
        self.lines = queue.Queue()
        env = os.environ.copy()
        env["JAVA_TOOL_OPTIONS"] = " ".join([
            f"-Ddustroute.instrumentation.capture_mode={capture_mode}",
            "-Ddustroute.instrumentation.pre_roll_ticks=2",
            "-Ddustroute.instrumentation.drain_ticks=2",
            "-Ddustroute.instrumentation.omit_heartbeats=false",
            f"-Ddustroute.instrumentation.output={raw_output}",
        ])
        # CaptureController treats any nonnegative max as bounded, even when
        # capture_mode says continuous. Do not silently trim construction data.
        if capture_mode == "bounded":
            env["JAVA_TOOL_OPTIONS"] += f" -Ddustroute.instrumentation.max_ticks_after_input={capture_ticks}"
        if trace_bounds is not None:
            assert len(trace_bounds) == 6 and all(type(v) is int for v in trace_bounds)
            env["JAVA_TOOL_OPTIONS"] += " -Ddustroute.instrumentation.trace_bounds=" + ",".join(map(str, trace_bounds))
        self.proc = subprocess.Popen(
            ["./gradlew", "runServer", "--console=plain", "--max-workers=1", "--no-parallel"], cwd=instrumentation,
            env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, bufsize=1,
        )

        def consume():
            for line in self.proc.stdout:
                self.console.write(line)
                self.console.flush()
                self.lines.put(line.rstrip())
            self.lines.put(None)

        self.reader = threading.Thread(target=consume, daemon=True)
        self.reader.start()
        try:
            self.until(lambda line: "Done (" in line, 90)
            with raw_output.open() as raw:
                header = json.loads(raw.readline())
            assert header["capture_mode"] == capture_mode, "server capture mode differs from requested mode"
        except BaseException:
            self.close()
            raise

    def until(self, predicate, timeout):
        deadline = time.monotonic() + timeout
        retained = []
        while True:
            line = self.lines.get(timeout=max(0.001, deadline - time.monotonic()))
            if line is None:
                raise RuntimeError("instrumented server stopped before becoming ready")
            retained.append(line)
            if predicate(line):
                return retained

    def close(self):
        error = None
        if self.proc.poll() is None:
            self.proc.stdin.write("stop\n")
            self.proc.stdin.flush()
            try:
                self.proc.wait(timeout=45)
            except subprocess.TimeoutExpired:
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
                    self.proc.wait(timeout=5)
                error = RuntimeError("instrumented server did not stop normally")
        self.reader.join(timeout=5)
        self.console.close()
        if self.proc.returncode != 0 and error is None:
            error = RuntimeError(f"instrumented server exited with {self.proc.returncode}")
        if error:
            raise error


def ensure_private_server(instrumentation):
    properties = (instrumentation / "run/server.properties").read_text()
    assert re.search(r"^online-mode=false$", properties, re.M), "offline private server required"
    assert re.search(r"^server-port=25565$", properties, re.M), "expected private port 25565"
    assert re.search(r"^eula=true$", (instrumentation / "run/eula.txt").read_text(), re.M), "existing EULA acceptance required"
    check = subprocess.run(["bash", "-lc", "exec 3<>/dev/tcp/127.0.0.1/25565"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert check.returncode != 0, "server port 25565 must be unused"
