// CONFIG
// const METRICS_URL = 'http://localhost:4011/health-metrics';
const METRICS_URL = 'http://185.7.81.99:4011/health-metrics';
const POLL_MS = 1000;
const EXCHANGES = ['binance', 'bybit', 'okx', 'gate', 'bitget', 'whitebit', /*'crypto'*/];

// We will explicitly track these:
const TRACKED_METRICS = {
  ws_server_packages_sent_counter: {
    label: 'WS server: packages sent',
    unit: 'count',
  },
  ws_clients_packages_received_counter: {
    label: 'WS client: packages received',
    unit: 'count',
  },
};

// add exchange metrics
EXCHANGES.forEach(ex => {
  TRACKED_METRICS[`${ex}_updates_received_counter`] = {
    label: `${ex.toUpperCase()} updates received`,
    unit: 'count',
  };
});

const metricsState = {}; // name -> {last, chart}
const charts = {}; // name -> {uplot, times, values}
const cardsGrid = document.getElementById('cardsGrid');

function createMetricCard(name, label) {
  const card = document.createElement('div');
  card.className = 'card';
  card.id = `card-${name}`;

  card.innerHTML = `
        <div class="card-title">
          <span>${label}</span>
          <span style="font-size:.65rem; color:var(--muted);">${name}</span>
        </div>
        <div class="card-value" id="card-${name}-value">N/A</div>
        <div class="card-sub" id="card-${name}-delta">Δ/s: N/A</div>
        <div class="mini-chart" id="chart-${name}"></div>
      `;
  return card;
}

function createChart(name, label) {
  const container = document.getElementById(`chart-${name}`);
  // We'll create a tiny uPlot
  const times = [0];
  const values = [0];

  const opts = {
    width: container.clientWidth || 240,
    height: 400,
    pxAlign: 0,
    scales: {
      x: { time: false },
      y: { auto: true },
    },
    series: [
      {},
      {
        label,
        stroke: "rgba(56,189,248,1)",
        width: 1.5,
      }
    ],
    axes: [
      { stroke: "white" },
      { stroke: "white" },
    ],
    legend: {
      show: false,
    }
  };

  const u = new uPlot(opts, [times, values], container);
  charts[name] = {
    uplot: u,
    times,
    values,
    start: Date.now(),
  };
}

// build UI upfront
Object.entries(TRACKED_METRICS).forEach(([name, cfg]) => {
  const card = createMetricCard(name, cfg.label);
  cardsGrid.appendChild(card);

  // Also add a bigger chart on the right panel
  const chartBlock = document.createElement('div');
  chartBlock.className = 'chart-block';
  chartBlock.innerHTML = `
        <div class="chart-title">${cfg.label}</div>
      `;
});

Object.entries(TRACKED_METRICS).forEach(([name, cfg]) => {
  createChart(name, cfg.label);
});

async function loadMetrics() {
  const statusEl = document.getElementById('status');
  try {
    const resp = await fetch(METRICS_URL, { cache: 'no-cache' });
    const text = await resp.text();
    const lines = text.split('\n');
    const tbody = document.querySelector('#metricsTable tbody');
    tbody.innerHTML = '';

    const now = new Date();
    statusEl.innerHTML = `<span class="status-dot" style="background: var(--success);"></span> last fetch: ${now.toLocaleTimeString()}`;

    // raw metrics to table + process
    const raw = {};
    lines.forEach(line => {
      if (!line.trim() || line.startsWith('#')) return;
      const [metric, value] = line.trim().split(/\s+/);
      raw[metric] = +value;
      const tr = document.createElement('tr');
      tr.innerHTML = `<td>${metric}</td><td>${value}</td>`;
      tbody.appendChild(tr);
    });

    // update tracked metrics
    Object.keys(TRACKED_METRICS).forEach(name => {
      const currentVal = raw[name];
      const cardValEl = document.getElementById(`card-${name}-value`);
      const cardDeltaEl = document.getElementById(`card-${name}-delta`);

      if (typeof currentVal === 'number' && !isNaN(currentVal)) {
        const prev = metricsState[name]?.last;
        const delta = typeof prev === 'number' ? currentVal - prev : 0;
        metricsState[name] = { last: currentVal, lastDelta: delta };

        cardValEl.textContent = `${currentVal.toLocaleString()} Current: ${delta}`;
        cardDeltaEl.textContent = `Δ/s: ${delta >= 0 ? delta : 0}`;

        pushToChart(name, delta >= 0 ? delta : 0);
      } else {
        cardValEl.textContent = 'N/A';
        cardDeltaEl.textContent = 'Δ/s: N/A';

        pushToChart(name, 0);
      }
    });

  } catch (err) {
    console.error(err);
    statusEl.innerHTML = `<span class="status-dot" style="background: var(--danger);"></span> fetch error`;
  }
}

function pushToChart(name, value) {
  const c = charts[name];
  if (!c || !c.uplot) return;

  const elapsed = Math.floor((Date.now() - c.start) / 1000);
  c.times.push(elapsed);
  c.values.push(value);

  if (c.times.length > 60) {
    c.times.shift();
    c.values.shift();
  }
  c.uplot.setData([c.times, c.values]);
}

document.getElementById('refreshBtn').addEventListener('click', () => {
  loadMetrics();
});

loadMetrics();
setInterval(loadMetrics, POLL_MS);