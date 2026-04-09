pub fn html() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>canplayer_plus</title>
  <style>
    :root {
      --bg: #0f1417;
      --panel: #172027;
      --panel-2: #22303a;
      --text: #eef5f2;
      --muted: #97a7a3;
      --accent: #f2b84b;
      --accent-2: #7cd1b8;
      --danger: #ff7c69;
      --border: rgba(255, 255, 255, 0.08);
      --shadow: rgba(0, 0, 0, 0.28);
    }

    * {
      box-sizing: border-box;
    }

    body {
      margin: 0;
      min-height: 100vh;
      font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      color: var(--text);
      background:
        radial-gradient(circle at top left, rgba(242, 184, 75, 0.22), transparent 28%),
        radial-gradient(circle at bottom right, rgba(124, 209, 184, 0.18), transparent 30%),
        linear-gradient(160deg, #0b1013 0%, #11191d 45%, #0c1114 100%);
      display: grid;
      place-items: center;
      padding: 24px;
    }

    .shell {
      width: min(920px, 100%);
      background: linear-gradient(180deg, rgba(23, 32, 39, 0.96), rgba(16, 23, 28, 0.96));
      border: 1px solid var(--border);
      border-radius: 24px;
      box-shadow: 0 28px 70px var(--shadow);
      overflow: hidden;
    }

    .hero {
      padding: 28px 28px 18px;
      border-bottom: 1px solid var(--border);
      background:
        linear-gradient(120deg, rgba(242, 184, 75, 0.16), transparent 50%),
        linear-gradient(220deg, rgba(124, 209, 184, 0.14), transparent 40%);
    }

    h1 {
      margin: 0;
      font-size: clamp(2rem, 4vw, 3rem);
      letter-spacing: -0.05em;
      font-weight: 700;
    }

    .subtitle {
      margin-top: 8px;
      color: var(--muted);
      max-width: 60ch;
      line-height: 1.45;
    }

    .content {
      padding: 28px;
      display: grid;
      gap: 20px;
    }

    .panel {
      background: linear-gradient(180deg, rgba(34, 48, 58, 0.85), rgba(23, 32, 39, 0.92));
      border: 1px solid var(--border);
      border-radius: 18px;
      padding: 20px;
    }

    .grid {
      display: grid;
      grid-template-columns: 1.2fr 0.8fr;
      gap: 20px;
    }

    .label-row,
    .meta-row {
      display: flex;
      justify-content: space-between;
      gap: 16px;
      align-items: center;
      flex-wrap: wrap;
    }

    .eyebrow {
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.14em;
      font-size: 0.76rem;
      margin-bottom: 10px;
    }

    .time {
      font-size: clamp(1.9rem, 4vw, 3rem);
      font-weight: 700;
      letter-spacing: -0.06em;
    }

    .muted {
      color: var(--muted);
    }

    input[type="range"] {
      width: 100%;
      margin: 18px 0 10px;
      accent-color: var(--accent);
    }

    .button-row {
      display: flex;
      gap: 10px;
      flex-wrap: wrap;
      margin-top: 14px;
    }

    button,
    input[type="number"] {
      border: 1px solid var(--border);
      border-radius: 12px;
      font: inherit;
    }

    button {
      padding: 11px 16px;
      color: var(--text);
      background: var(--panel-2);
      cursor: pointer;
      transition: transform 140ms ease, background 140ms ease, border-color 140ms ease;
    }

    button:hover {
      transform: translateY(-1px);
      border-color: rgba(242, 184, 75, 0.35);
    }

    button.primary {
      background: linear-gradient(135deg, #f2b84b, #d98d2b);
      color: #1a1610;
      font-weight: 700;
    }

    button.ghost {
      background: transparent;
    }

    button.danger {
      color: var(--danger);
    }

    .speed-box {
      display: flex;
      gap: 10px;
      align-items: center;
      margin-top: 14px;
    }

    input[type="number"] {
      width: 110px;
      padding: 11px 12px;
      background: rgba(12, 17, 20, 0.7);
      color: var(--text);
    }

    .status-card {
      display: grid;
      gap: 14px;
    }

    .pill {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      border-radius: 999px;
      padding: 8px 12px;
      background: rgba(124, 209, 184, 0.1);
      color: var(--accent-2);
      font-weight: 600;
      width: fit-content;
    }

    .pill.paused {
      background: rgba(242, 184, 75, 0.12);
      color: var(--accent);
    }

    dl {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 12px;
      margin: 0;
    }

    dt {
      color: var(--muted);
      font-size: 0.84rem;
      margin-bottom: 4px;
    }

    dd {
      margin: 0;
      font-weight: 600;
    }

    pre {
      margin: 0;
      padding: 14px;
      border-radius: 14px;
      background: rgba(9, 13, 16, 0.82);
      color: #d8ece5;
      font-size: 0.92rem;
      overflow: auto;
    }

    .error {
      color: var(--danger);
      min-height: 1.4em;
      font-size: 0.95rem;
    }

    @media (max-width: 760px) {
      .grid {
        grid-template-columns: 1fr;
      }

      .hero,
      .content {
        padding: 20px;
      }

      dl {
        grid-template-columns: 1fr;
      }
    }
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <h1>canplayer_plus</h1>
      <div class="subtitle">
        Seekable CAN replay with a live control plane. Scrub the timeline, single-step frames, change speed, and watch the next transmit target update in real time.
      </div>
    </section>

    <section class="content">
      <div class="grid">
        <div class="panel">
          <div class="eyebrow">Transport</div>
          <div class="label-row">
            <div class="time"><span id="currentMs">0</span> ms</div>
            <div class="muted">of <span id="durationMs">0</span> ms</div>
          </div>

          <input id="timeline" type="range" min="0" max="0" value="0" step="1">

          <div class="meta-row muted">
            <span>cursor <span id="cursorIndex">0</span></span>
            <span>frames <span id="frameCount">0</span></span>
            <span>speed <span id="speedReadout">1.00</span>x</span>
          </div>

          <div class="button-row">
            <button id="playBtn" class="primary">Play</button>
            <button id="pauseBtn">Pause</button>
            <button id="backBtn" class="ghost">Step -1</button>
            <button id="stepBtn" class="ghost">Step +1</button>
            <button id="endBtn" class="ghost">Jump End</button>
          </div>

          <div class="speed-box">
            <input id="speedInput" type="number" min="0.05" step="0.05" value="1.0">
            <button id="speedBtn">Apply Speed</button>
            <button id="refreshBtn" class="ghost">Refresh</button>
            <button id="quitBtn" class="danger ghost">Quit</button>
          </div>

          <div id="error" class="error"></div>
        </div>

        <aside class="panel status-card">
          <div class="eyebrow">Status</div>
          <div id="playState" class="pill paused">Paused</div>

          <dl>
            <div>
              <dt>Next frame</dt>
              <dd id="nextFrame">none</dd>
            </div>
            <div>
              <dt>Interface</dt>
              <dd id="nextIface">-</dd>
            </div>
            <div>
              <dt>Length</dt>
              <dd id="nextLen">-</dd>
            </div>
            <div>
              <dt>Mode</dt>
              <dd id="nextMode">-</dd>
            </div>
          </dl>

          <pre id="rawStatus">{}</pre>
        </aside>
      </div>
    </section>
  </main>

  <script>
    const timeline = document.getElementById("timeline");
    const currentMs = document.getElementById("currentMs");
    const durationMs = document.getElementById("durationMs");
    const cursorIndex = document.getElementById("cursorIndex");
    const frameCount = document.getElementById("frameCount");
    const speedReadout = document.getElementById("speedReadout");
    const speedInput = document.getElementById("speedInput");
    const playState = document.getElementById("playState");
    const nextFrame = document.getElementById("nextFrame");
    const nextIface = document.getElementById("nextIface");
    const nextLen = document.getElementById("nextLen");
    const nextMode = document.getElementById("nextMode");
    const rawStatus = document.getElementById("rawStatus");
    const errorBox = document.getElementById("error");

    let dragging = false;
    let pollTimer = null;

    function setError(message) {
      errorBox.textContent = message || "";
    }

    async function api(path, options = {}) {
      const response = await fetch(path, options);
      const text = await response.text();
      let payload = {};
      try {
        payload = text ? JSON.parse(text) : {};
      } catch (_) {
        payload = { raw: text };
      }

      if (!response.ok) {
        throw new Error(payload.error || ("request failed: " + response.status));
      }
      return payload;
    }

    function applyStatus(status) {
      currentMs.textContent = status.current_ms;
      durationMs.textContent = status.duration_ms;
      cursorIndex.textContent = status.cursor_index;
      frameCount.textContent = status.total_frames;
      speedReadout.textContent = Number(status.speed).toFixed(2);
      speedInput.value = Number(status.speed).toFixed(2);

      timeline.max = String(status.duration_ms);
      if (!dragging) {
        timeline.value = String(status.current_ms);
      }

      if (status.playing) {
        playState.textContent = "Playing";
        playState.classList.remove("paused");
      } else {
        playState.textContent = "Paused";
        playState.classList.add("paused");
      }

      if (status.next_frame) {
        nextFrame.textContent = status.next_frame.can_id;
        nextIface.textContent = status.next_frame.iface;
        nextLen.textContent = String(status.next_frame.len);
        nextMode.textContent = status.next_frame.fd ? "CAN FD" : "Classic CAN";
      } else {
        nextFrame.textContent = "none";
        nextIface.textContent = "-";
        nextLen.textContent = "-";
        nextMode.textContent = "-";
      }

      rawStatus.textContent = JSON.stringify(status, null, 2);
      setError(status.last_error || "");
    }

    async function refreshStatus() {
      try {
        const status = await api("/status");
        applyStatus(status);
      } catch (error) {
        setError(error.message);
      }
    }

    async function post(path) {
      try {
        const status = await api(path, { method: "POST" });
        if (status && status.current_ms !== undefined) {
          applyStatus(status);
        } else {
          setError("");
        }
      } catch (error) {
        setError(error.message);
      }
    }

    document.getElementById("playBtn").addEventListener("click", () => post("/play"));
    document.getElementById("pauseBtn").addEventListener("click", () => post("/pause"));
    document.getElementById("backBtn").addEventListener("click", () => post("/step?count=-1"));
    document.getElementById("stepBtn").addEventListener("click", () => post("/step?count=1"));
    document.getElementById("refreshBtn").addEventListener("click", refreshStatus);
    document.getElementById("endBtn").addEventListener("click", () => {
      post("/seek?ms=" + encodeURIComponent(timeline.max || "0"));
    });
    document.getElementById("quitBtn").addEventListener("click", async () => {
      await post("/quit");
      setError("server requested shutdown");
      window.clearInterval(pollTimer);
    });
    document.getElementById("speedBtn").addEventListener("click", () => {
      post("/speed?value=" + encodeURIComponent(speedInput.value));
    });

    timeline.addEventListener("pointerdown", () => {
      dragging = true;
    });

    timeline.addEventListener("pointerup", async () => {
      dragging = false;
      await post("/seek?ms=" + encodeURIComponent(timeline.value));
      await refreshStatus();
    });

    timeline.addEventListener("change", () => {
      if (!dragging) {
        post("/seek?ms=" + encodeURIComponent(timeline.value));
      }
    });

    timeline.addEventListener("input", () => {
      if (dragging) {
        currentMs.textContent = timeline.value;
      }
    });

    refreshStatus();
    pollTimer = window.setInterval(refreshStatus, 250);
  </script>
</body>
</html>
"#
}
