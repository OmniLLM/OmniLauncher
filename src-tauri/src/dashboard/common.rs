//! Shared helpers and HTML layout for dashboard pages.

use crate::{db, path_config};
use rusqlite::Connection;

pub fn open_db() -> rusqlite::Result<Connection> {
    let dir = path_config::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let conn = Connection::open(dir.join("omnilauncher.sqlite"))?;
    db::run_migrations(&conn)?;
    Ok(conn)
}

pub fn count_query(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0)
}

pub fn collect_kv(conn: &Connection, sql: &str) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(sql) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        }) {
            for row in rows.flatten() {
                out.push(row);
            }
        }
    }
    out
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn now_human() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Shared CSS variables, base layout, top nav. The page-specific body html
/// and the data-loading script are injected by each page.
///
/// `{title}`, `{active}`, `{body}`, `{script}` are placeholder tokens replaced
/// at render time. No `format!` is used to avoid `{}` escaping issues with the
/// inline JS.
pub fn render_page(title: &str, active: &str, body: &str, script: &str) -> String {
    LAYOUT
        .replace("{{TITLE}}", title)
        .replace("{{ACTIVE}}", active)
        .replace("{{BODY}}", body)
        .replace("{{SCRIPT}}", script)
}

const LAYOUT: &str = r##"<!doctype html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1" />
<title>OmniLauncher · {{TITLE}}</title>
<script>
  // Apply persisted theme synchronously before paint to avoid FOUC.
  (function() {
    try {
      var t = localStorage.getItem('omnilauncher.dashboard.theme') || 'dark';
      document.documentElement.setAttribute('data-theme', t);
    } catch (e) { /* localStorage unavailable */ }
  })();
</script>
<script src="https://cdn.tailwindcss.com"></script>
<script>
  tailwind.config = {
    darkMode: 'class',
    theme: {
      extend: {
        fontFamily: {
          sans: ['Inter', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'Roboto', 'sans-serif'],
          mono: ['ui-monospace', '"Cascadia Code"', 'JetBrains Mono', 'monospace'],
        },
        colors: {
          /* ink-* remapped to match the app's Google-style dark palette */
          ink: {
            900: '#202124',  /* --bg           */
            800: '#292a2d',  /* --bg-elevated   */
            700: '#303134',  /* --surface       */
            600: '#3c3f43',  /* --surface-2     */
            500: '#45474b',  /* --surface-hover */
          },
          sky: {
            /* override sky-400 → app accent #8ab4f8; keep surrounding shades proportional */
            300: '#c2d7ff',
            400: '#8ab4f8',
            500: '#669df6',
            700: '#3c4f8a',
            900: '#1a2644',
          },
        },
        boxShadow: {
          card: '0 1px 3px rgba(0,0,0,0.3), 0 4px 16px rgba(0,0,0,0.4)',
          glow: '0 0 0 1px rgba(138,180,248,0.15), 0 10px 40px -10px rgba(138,180,248,0.25)',
        },
        keyframes: {
          pulseDot: {
            '0%':   { boxShadow: '0 0 0 0 rgba(52,168,83,0.6)' },
            '70%':  { boxShadow: '0 0 0 10px rgba(52,168,83,0)' },
            '100%': { boxShadow: '0 0 0 0 rgba(52,168,83,0)' },
          },
        },
        animation: { pulseDot: 'pulseDot 1.8s infinite' },
      },
    },
  }
</script>
<style type="text/tailwindcss">
  /* ---------- CSS variables — exact match with the front-end app's styles.css ---------- */
  :root {
    --bg:           #202124;
    --bg-elevated:  #292a2d;
    --surface:      #303134;
    --surface-2:    #3c3f43;
    --surface-hover:#45474b;
    --text:         #e8eaed;
    --text-secondary:#bdc1c6;
    --sub:          #9aa0a6;
    --accent:       #8ab4f8;
    --accent-dim:   rgba(138,180,248,0.15);
    --accent-hover: rgba(138,180,248,0.08);
    --border:       rgba(255,255,255,0.12);
    --shadow:       0 1px 3px rgba(0,0,0,0.3), 0 4px 16px rgba(0,0,0,0.4);
    --success:      #34a853;
    --error:        #ea4335;
    --radius:       8px;
    /* legacy aliases */
    --bg-2:      #292a2d;
    --green:     #34a853;
    --yellow:    #fbbc04;
    --red:       #ea4335;
    --purple:    #a78bfa;
    --cyan:      #22d3ee;
  }

  @layer base {
    * { box-sizing: border-box; }
    html, body { @apply m-0 p-0 min-h-screen antialiased font-sans; }
    body {
      background: #202124;
      color: #e8eaed;
    }
    h1, h2, h3, h4 { color: #e8eaed; }
    a { @apply transition-colors; }
    code { @apply font-mono text-[12px] px-1.5 py-0.5 rounded bg-white/5 text-sky-400; }
  }

  @layer components {
    /* ---------- layout shell ---------- */
    nav.topbar { @apply sticky top-0 z-20 px-4 sm:px-8 py-3 border-b flex items-center gap-5 flex-wrap;
                 background: #292a2d; border-color: rgba(255,255,255,0.12); }
    .nav-container { @apply max-w-[1400px] w-full mx-auto flex items-center gap-5 flex-wrap; }
    nav.topbar .brand { @apply font-bold text-[15px] tracking-tight; color: #e8eaed; }
    nav.topbar .brand .accent { color: #8ab4f8; }
    nav.topbar .links { @apply flex gap-1 flex-wrap; }
    nav.topbar a {
      @apply no-underline px-3 py-1.5 rounded-lg text-[13px] font-medium transition-colors;
      color: #9aa0a6;
    }
    nav.topbar a:hover { background: rgba(255,255,255,0.05); color: #e8eaed; }
    nav.topbar a.active {
      background: rgba(255,255,255,0.05);
      color: #e8eaed;
      box-shadow: inset 0 -2px 0 #8ab4f8;
    }
    nav.topbar .right { @apply ml-auto text-xs flex items-center gap-3; color: #9aa0a6; }
    nav.topbar .pulse {
      @apply inline-block w-2 h-2 rounded-full mr-2 align-middle animate-pulseDot;
      background: #34a853;
    }
    /* theme toggle in the topbar */
    nav.topbar #theme-toggle {
      @apply inline-flex items-center justify-center w-8 h-8 rounded-lg border cursor-pointer transition;
      background: rgba(255,255,255,0.04);
      border-color: rgba(255,255,255,0.12);
      color: #e8eaed;
      font-size: 15px;
      line-height: 1;
    }
    nav.topbar #theme-toggle:hover {
      background: rgba(255,255,255,0.10);
      border-color: rgba(255,255,255,0.25);
    }
    nav.topbar #theme-toggle .theme-icon { line-height: 1; }

    main { @apply max-w-[1400px] mx-auto px-4 sm:px-8 pt-6 pb-16; }
    h1.page-title { @apply text-2xl font-bold mb-6 tracking-tight; color: #e8eaed; }

    /* ---------- stat cards ---------- */
    .grid-stats { @apply grid gap-3.5 mb-6;
                  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); }
    .stat {
      @apply rounded-lg border p-4 shadow-card transition hover:-translate-y-0.5;
      background: #303134; border-color: rgba(255,255,255,0.12);
    }
    .stat .label { @apply text-[11px] uppercase tracking-widest font-semibold mb-1.5; color: #9aa0a6; }
    .stat .value { @apply text-3xl font-bold leading-tight; color: #e8eaed; }
    .stat .sub   { @apply text-xs mt-1; color: #9aa0a6; }
    .stat.green  .value { color: #34a853; }
    .stat.yellow .value { color: #fbbc04; }
    .stat.red    .value { color: #ea4335; }
    .stat.accent .value { color: #8ab4f8; }
    .stat.purple .value { color: #a78bfa; }
    .stat.cyan   .value { color: #22d3ee; }

    /* ---------- main grid + cards ---------- */
    .grid-main { @apply grid gap-4;
                 grid-template-columns: repeat(auto-fit, minmax(360px, 1fr)); }
    .card {
      @apply rounded-lg border p-5 shadow-card;
      background: #303134; border-color: rgba(255,255,255,0.12);
    }
    .card h2 {
      @apply text-[12px] uppercase tracking-widest mb-3 font-semibold; color: #9aa0a6;
    }

    /* ---------- progress bars ---------- */
    .bar-row { @apply flex items-center gap-2.5 mb-2 text-[13px]; }
    .bar-row .label { @apply min-w-[110px]; color: #e8eaed; }
    .bar-row .count { @apply min-w-[36px] text-right tabular-nums; color: #9aa0a6; }
    .bar { @apply flex-1 h-2 rounded-full overflow-hidden; background: rgba(255,255,255,0.08); }
    .bar > span { @apply block h-full rounded-full transition-all duration-500; }
    .bar.todo        > span { background: #5f6368; }
    .bar.in_progress > span { background: #8ab4f8; }
    .bar.blocked     > span { background: #ea4335; }
    .bar.done        > span { background: #34a853; }

    /* ---------- charts ---------- */
    .chart { @apply flex items-end gap-1 h-24 pt-1.5; }
    .chart .col { @apply flex-1 flex flex-col items-stretch justify-end min-w-[8px]; }
    .chart .col > span {
      @apply block rounded-t min-h-[2px] transition-all duration-500;
      background: linear-gradient(to bottom, #8ab4f8, #3c4f8a);
    }
    .chart .col.b > span { background: linear-gradient(to bottom, #34a853, #1a5e36); }
    .chart-axis { @apply flex gap-1 mt-1.5 text-[10px]; color: #9aa0a6; }
    .chart-axis .col { @apply flex-1 text-center min-w-[8px] overflow-hidden whitespace-nowrap; }

    /* ---------- lists ---------- */
    .list { @apply flex flex-col gap-2; }
    .list .item {
      @apply flex items-center gap-2.5 px-3 py-2 rounded-lg text-[13px] border transition;
      background: #292a2d; border-color: rgba(255,255,255,0.12);
    }
    .list .item:hover { border-color: rgba(255,255,255,0.2); }
    .list .item .id { @apply text-[11px] font-mono min-w-[36px]; color: #9aa0a6; }
    .list .item .body { @apply flex-1 overflow-hidden text-ellipsis whitespace-nowrap; color: #e8eaed; }
    .list .item .meta { @apply text-[11px]; color: #9aa0a6; }

    /* ---------- badges (status) ---------- */
    .badge {
      @apply inline-block px-2 py-0.5 rounded-full text-[10px] font-semibold
             tracking-wider uppercase border border-transparent;
    }
    .badge.todo        { background: rgba(95,99,104,0.4);  color: #bdc1c6; }
    .badge.in_progress { background: rgba(138,180,248,0.15); color: #8ab4f8; border-color: rgba(138,180,248,0.4); }
    .badge.blocked     { background: rgba(234,67,53,0.15);  color: #f28b82; border-color: rgba(234,67,53,0.4); }
    .badge.done        { background: rgba(52,168,83,0.15);  color: #81c995; border-color: rgba(52,168,83,0.4); }

    /* ---------- priority pills ---------- */
    .pri { @apply text-[11px] font-semibold px-1.5 py-px rounded border; }
    .pri-1 { color: #f28b82; background: rgba(234,67,53,0.15);  border-color: rgba(234,67,53,0.4); }
    .pri-2 { color: #fdd663; background: rgba(251,188,4,0.15);  border-color: rgba(251,188,4,0.4); }
    .pri-3 { color: #bdc1c6; background: rgba(95,99,104,0.3);   border-color: rgba(95,99,104,0.5); }
    .pri-4 { color: #8ab4f8; background: rgba(138,180,248,0.12); border-color: rgba(138,180,248,0.4); }
    .pri-5 { color: #81c995; background: rgba(52,168,83,0.12);  border-color: rgba(52,168,83,0.4); }

    /* ---------- tags ---------- */
    .tag-cloud { @apply flex flex-wrap gap-1.5; }
    .tag {
      @apply inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] border;
      background: #292a2d; border-color: rgba(255,255,255,0.12); color: #bdc1c6;
    }
    .tag .c { color: #8ab4f8; font-weight: bold; }

    /* ---------- messages ---------- */
    .msg {
      @apply flex flex-col gap-1 px-3 py-2.5 rounded-lg text-[12.5px] border;
      background: #292a2d; border-color: rgba(255,255,255,0.12);
    }
    .msg .head { @apply flex justify-between text-[11px]; color: #9aa0a6; }
    .msg.user      { border-left: 3px solid #8ab4f8; }
    .msg.assistant { border-left: 3px solid #a78bfa; }
    .msg .content {
      color: #e8eaed; line-height: 1.6;
      overflow: hidden; white-space: pre-wrap;
      display: -webkit-box; -webkit-line-clamp: 6; -webkit-box-orient: vertical;
    }

    /* ---------- misc ---------- */
    .empty { @apply text-xs py-2.5 px-1; color: #9aa0a6; }

    .donut-wrap { @apply flex items-center gap-4; }
    .donut { @apply w-28 h-28 flex-shrink-0 -rotate-90; }
    .donut circle { fill: none; stroke-width: 14; }
    .donut .track { stroke: rgba(255,255,255,0.08); }
    .donut-center { @apply flex flex-col items-start; }
    .donut-center .pct { @apply text-3xl font-bold; color: #e8eaed; }
    .donut-center .lbl { @apply text-[11px] uppercase tracking-widest; color: #9aa0a6; }
    .legend { @apply flex flex-col gap-1.5 text-xs; }
    .legend .item { @apply flex items-center gap-2; color: #bdc1c6; }
    .legend .item .dot { @apply w-2.5 h-2.5 rounded-full; }

    footer { @apply my-8 mx-8 text-center text-[11px]; color: #9aa0a6; }

    /* ---------- index page: dashboard cards grid ---------- */
    .dash-grid { @apply grid gap-4;
                 grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); }
    .dash-card {
      @apply relative flex flex-col gap-3 p-6 rounded-lg no-underline border
             shadow-card overflow-hidden transition hover:-translate-y-1;
      background: #303134; border-color: rgba(255,255,255,0.12); color: #e8eaed;
    }
    .dash-card:hover { border-color: rgba(255,255,255,0.25); }
    .dash-card::before {
      content: ""; position: absolute; inset: 0;
      background: radial-gradient(400px 200px at 100% 0%, var(--card-glow, rgba(138,180,248,0.10)), transparent 70%);
      pointer-events: none;
    }
    .dash-card > * { @apply relative; }
    .dash-card .head { @apply flex items-center gap-3; }
    .dash-card .icon {
      @apply w-10 h-10 flex-shrink-0 rounded-lg inline-flex items-center justify-center
             text-xl font-bold text-white shadow-lg;
      background: var(--card-accent, #8ab4f8);
    }
    .dash-card .title { @apply text-base font-bold; color: #e8eaed; }
    .dash-card .desc { @apply text-[12.5px] leading-relaxed; color: #9aa0a6; }
    .dash-card .metrics { @apply flex gap-5 mt-1; }
    .dash-card .metric { @apply flex flex-col gap-0.5; }
    .dash-card .metric .v {
      @apply text-2xl font-bold tabular-nums;
      color: var(--card-accent, #8ab4f8);
    }
    .dash-card .metric .k {
      @apply text-[10.5px] uppercase tracking-widest; color: #9aa0a6;
    }
    .dash-card .cta {
      @apply mt-auto pt-1.5 text-xs flex items-center gap-1.5; color: #9aa0a6;
    }
    .dash-card .cta .arr { @apply transition-transform; }
    .dash-card:hover .cta { color: #e8eaed; }
    .dash-card:hover .cta .arr { @apply translate-x-1; }

    /* ---------- conversation page: collapsible session ---------- */
    .session {
      @apply rounded-lg border overflow-hidden transition;
      background: #292a2d; border-color: rgba(255,255,255,0.12);
    }
    .session:hover { border-color: rgba(255,255,255,0.2); }
    .session.open { border-color: rgba(138,180,248,0.5); }
    .session-head {
      @apply flex items-center gap-2.5 px-4 py-3 cursor-pointer select-none;
    }
    .session-head:hover { background: rgba(255,255,255,0.03); }
    .session-head .caret { @apply transition-transform; color: #9aa0a6; }
    .session.open .session-head .caret { transform: rotate(90deg); color: #8ab4f8; }
    .session-head .num { @apply font-mono text-[11px] min-w-[28px]; color: #9aa0a6; }
    .session-head .title {
      @apply flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-semibold text-[13.5px];
    }
    .session-head .pill {
      @apply text-[10.5px] px-2 py-0.5 rounded-full border;
      background: rgba(255,255,255,0.05); border-color: rgba(255,255,255,0.12); color: #9aa0a6;
    }
    .session-head .pill.u { color: #8ab4f8; border-color: rgba(138,180,248,0.3); }
    .session-head .pill.a { color: #a78bfa; border-color: rgba(167,139,250,0.3); }
    .session-head .when { @apply text-[11px]; color: #9aa0a6; }
    .session-body {
      @apply px-4 pt-1 pb-3.5 flex flex-col gap-2 border-t;
      border-color: rgba(255,255,255,0.12);
    }

    /* ---------- jobs page: row cards ---------- */
    .job-row {
      @apply px-4 py-3 rounded-lg border flex flex-col gap-2 transition;
      background: #292a2d; border-color: rgba(255,255,255,0.12);
    }
    .job-row:hover { border-color: rgba(255,255,255,0.2); }
    .job-row .row-head { @apply flex items-center gap-2.5 flex-wrap; }
    .job-row .row-head .id { @apply font-mono text-[11px]; color: #9aa0a6; }
    .job-row .row-head .label { @apply font-semibold; color: #e8eaed; }
    .job-row .row-head .meta { @apply ml-auto text-[11px]; color: #9aa0a6; }
    .job-row .row-grid {
      @apply grid gap-2 text-xs; color: #bdc1c6;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    }
    .job-row .row-grid .k { @apply mr-1; color: #9aa0a6; }
    .job-row .row-grid .rel { @apply ml-1 text-[11px]; color: #9aa0a6; opacity: 0.8; }
    .job-row .cmd {
      @apply text-xs px-2.5 py-2 rounded font-mono break-all;
      background: rgba(255,255,255,0.05); color: #e8eaed;
    }

    /* ---------- tables page: table cards ---------- */
    .tbl-grid {
      @apply grid gap-3.5;
      grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    }
    .tbl-card {
      @apply rounded-lg border p-4 shadow-card flex flex-col gap-2.5 transition;
      background: #303134; border-color: rgba(255,255,255,0.12);
    }
    .tbl-card:hover { border-color: rgba(255,255,255,0.2); }
    .tbl-card .head { @apply flex items-baseline justify-between gap-2; }
    .tbl-card .name { @apply font-mono font-semibold text-sm; color: #8ab4f8; }
    .tbl-card .rows { @apply text-[11px] tabular-nums; color: #9aa0a6; }
    .tbl-card .cols { @apply text-[10.5px] leading-snug break-words; color: #9aa0a6; }
    .tbl-card .samples { @apply flex flex-col gap-1; }
    .tbl-card .sample {
      @apply px-2 py-1.5 rounded border flex flex-col gap-0.5;
      background: rgba(255,255,255,0.04); border-color: rgba(255,255,255,0.08);
    }
    .tbl-card .sample .kv { @apply flex gap-1.5 text-[11px] leading-snug; }
    .tbl-card .sample .kv .k { @apply min-w-[80px] flex-shrink-0; color: #9aa0a6; }
    .tbl-card .sample .kv .v {
      @apply overflow-hidden text-ellipsis whitespace-nowrap; color: #e8eaed;
    }
    .page-lede { @apply -mt-3 mb-6 text-[13px]; color: #9aa0a6; }

    /* ─────────────────────────────────────────────────────────────────────
       todos page (HTML produced by plugins/todo.rs `generate_html`).
       Class names below match what that template renders so it picks up
       the unified dashboard look without changes inside the plugin.
       ───────────────────────────────────────────────────────────────────── */

    /* stats strip at the top */
    .stats {
      @apply flex items-center gap-7 flex-wrap mb-6 px-5 py-4 rounded-lg border shadow-card;
      background: #303134; border-color: rgba(255,255,255,0.12);
    }
    .stats .stat { @apply bg-transparent border-0 shadow-none p-0 text-center; }
    .stats .stat .num { @apply text-2xl font-bold tabular-nums; color: #8ab4f8; }
    .stats .stat .lbl { @apply text-[11px] mt-0.5 uppercase tracking-wider; color: #9aa0a6; }
    .progress-wrap { @apply flex-1 min-w-[140px]; }
    .progress-lbl  { @apply text-[11px] mb-1.5; color: #9aa0a6; }
    .progress-bar  { @apply h-2 rounded-full overflow-hidden; background: rgba(255,255,255,0.08); }
    .progress-fill {
      @apply h-full rounded-full transition-all duration-500;
      background: linear-gradient(to right, #34a853, #81c995);
    }

    /* search + group toolbar */
    .toolbar { @apply flex items-center gap-2.5 mb-4 flex-wrap; }
    .toolbar label { @apply text-xs; color: #9aa0a6; }
    .toolbar select,
    .toolbar input {
      @apply border rounded-lg px-3 py-1.5 text-[13px] outline-none cursor-pointer transition;
      background: #292a2d; border-color: rgba(255,255,255,0.15); color: #e8eaed;
    }
    .toolbar select:focus,
    .toolbar input:focus  { border-color: #8ab4f8; box-shadow: 0 0 0 2px rgba(138,180,248,0.2); }
    .toolbar input { @apply flex-1 min-w-[160px] cursor-text; }

    /* todos table */
    #todo-table {
      @apply w-full border-collapse rounded-lg overflow-hidden border shadow-card;
      background: rgba(48,49,52,0.4); border-color: rgba(255,255,255,0.12);
    }
    #todo-table th {
      @apply relative px-3 py-2.5 text-[11px] font-semibold tracking-wider uppercase
             text-left cursor-pointer select-none whitespace-nowrap border-b transition;
      background: #292a2d; border-color: rgba(255,255,255,0.12); color: #9aa0a6;
    }
    #todo-table th:hover { color: #8ab4f8; }
    #todo-table th .arr { @apply ml-1 text-[10px]; opacity: 0.4; }
    #todo-table th.active .arr { opacity: 1; color: #8ab4f8; }
    #todo-table th .col-filter-badge {
      @apply inline-block ml-1.5 px-1.5 py-px rounded-full text-[9px] font-bold
             normal-case tracking-normal align-middle;
      background: #8ab4f8; color: #202124;
    }
    .col-dropdown {
      @apply absolute top-full left-0 z-50 min-w-[160px] hidden rounded-lg shadow-2xl overflow-hidden border;
      background: #292a2d; border-color: rgba(255,255,255,0.15);
    }
    .col-dropdown.open { @apply block; }
    .col-dropdown-item {
      @apply px-3.5 py-2 text-xs cursor-pointer whitespace-nowrap
             normal-case tracking-normal font-normal transition;
      color: #e8eaed;
    }
    .col-dropdown-item:hover { background: rgba(138,180,248,0.1); color: #8ab4f8; }
    .col-dropdown-item.selected { color: #81c995; font-weight: 600; }
    .col-dropdown-item.clear-item {
      color: #f28b82; border-top: 1px solid rgba(255,255,255,0.1); margin-top: 2px;
    }
    .col-dropdown-item.clear-item:hover { background: rgba(234,67,53,0.1); }

    tr.data-row { @apply cursor-pointer transition-colors; }
    tr.data-row:hover td { background: rgba(255,255,255,0.03); }
    tr.data-row.expanded td { background: rgba(138,180,248,0.06); border-bottom: 0; }

    /* ── Due-date warning tiers ─────────────────────────────────────────── */
    /* overdue:   red, blinking — past due date                              */
    /* due-today: orange, slow pulse — due within the next 24h               */
    /* due-soon:  yellow tint — due within 3 days                            */
    /* due-week:  blue tint   — due within 7 days                            */
    @keyframes overdue-blink {
      0%, 100% { background-color: rgba(242,139,130,0.22); }
      50%      { background-color: rgba(242,139,130,0.55); }
    }
    @keyframes today-pulse {
      0%, 100% { background-color: rgba(253,214,99,0.18); }
      50%      { background-color: rgba(253,214,99,0.38); }
    }
    tr.data-row.overdue td {
      animation: overdue-blink 1.2s ease-in-out infinite;
      border-bottom-color: rgba(242,139,130,0.5) !important;
      color: #fab8c8 !important;
    }
    tr.data-row.overdue:hover td   { animation: none; background: rgba(242,139,130,0.60); }
    tr.data-row.overdue.expanded td{ animation: none; background: rgba(242,139,130,0.45); }
    tr.data-row.overdue td.col-expand { box-shadow: inset 4px 0 0 #f28b82; }
    tr.data-row.overdue td.col-due    { color: #ffffff !important; font-weight: 700; }
    tr.data-row.overdue td.col-due::before { content: "⚠ "; font-size: 12px; }

    tr.data-row.due-today td {
      animation: today-pulse 2s ease-in-out infinite;
      border-bottom-color: rgba(253,214,99,0.5) !important;
      color: #fdd663 !important;
    }
    tr.data-row.due-today:hover td   { animation: none; background: rgba(253,214,99,0.42); }
    tr.data-row.due-today.expanded td{ animation: none; background: rgba(253,214,99,0.32); }
    tr.data-row.due-today td.col-expand { box-shadow: inset 4px 0 0 #fdd663; }
    tr.data-row.due-today td.col-due    { color: #fdd663 !important; font-weight: 700; }
    tr.data-row.due-today td.col-due::before { content: "🔥 "; font-size: 12px; }

    tr.data-row.due-soon td {
      background: rgba(249,226,175,0.10);
      border-bottom-color: rgba(249,226,175,0.3) !important;
    }
    tr.data-row.due-soon:hover td    { background: rgba(249,226,175,0.20); }
    tr.data-row.due-soon.expanded td { background: rgba(249,226,175,0.15); }
    tr.data-row.due-soon td.col-expand { box-shadow: inset 3px 0 0 #f9e2af; }
    tr.data-row.due-soon td.col-due    { color: #f9e2af !important; font-weight: 600; }
    tr.data-row.due-soon td.col-due::before { content: "⏳ "; font-size: 12px; }

    tr.data-row.due-week td {
      background: rgba(138,180,250,0.07);
      border-bottom-color: rgba(138,180,250,0.25) !important;
    }
    tr.data-row.due-week:hover td    { background: rgba(138,180,250,0.16); }
    tr.data-row.due-week.expanded td { background: rgba(138,180,250,0.12); }
    tr.data-row.due-week td.col-expand { box-shadow: inset 3px 0 0 #8ab4fa; }
    tr.data-row.due-week td.col-due    { color: #8ab4fa !important; font-weight: 600; }
    tr.data-row.due-week td.col-due::before { content: "📅 "; font-size: 12px; }
    #todo-table td {
      @apply px-3 py-2.5 text-[13.5px] align-middle border-b;
      border-color: rgba(255,255,255,0.08); color: #e8eaed;
    }
    td.col-expand { @apply w-6 text-xs pr-0; color: #9aa0a6; }
    td.col-id { @apply w-12 text-xs tabular-nums; color: #9aa0a6; }
    td.col-text { @apply leading-snug min-w-[180px]; }
    td.col-text .desc-preview { @apply text-[11px] mt-0.5 truncate max-w-[320px]; color: #9aa0a6; }
    td.col-text.done-text { color: #9aa0a6; text-decoration: line-through; }
    td.col-pri { @apply w-[100px] whitespace-nowrap; }
    td.col-status { @apply w-[100px] whitespace-nowrap; }
    td.col-due { @apply w-[98px] text-xs whitespace-nowrap; color: #9aa0a6; }
    td.col-tags { @apply w-[130px] text-xs; }
    td.col-date { @apply text-xs w-[94px] whitespace-nowrap; color: #9aa0a6; }
    td.col-completed { @apply text-xs w-[94px] whitespace-nowrap; color: #81c995; }
    td.col-comments { @apply w-16 text-xs text-center; color: #9aa0a6; }

    /* todo badges (hyphenated variants used by the todos page) */
    .badge-todo {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full border;
      background: rgba(95,99,104,0.4); color: #bdc1c6; border-color: rgba(95,99,104,0.5);
    }
    .badge-in-progress {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full border;
      background: rgba(138,180,248,0.12); color: #8ab4f8; border-color: rgba(138,180,248,0.35);
    }
    .badge-blocked {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full border;
      background: rgba(251,188,4,0.12); color: #fdd663; border-color: rgba(251,188,4,0.35);
    }
    .badge-done {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full border;
      background: rgba(52,168,83,0.12); color: #81c995; border-color: rgba(52,168,83,0.35);
    }

    /* todos page priority colors */
    .pri-1 { color: #f28b82; }
    .pri-2 { color: #fdd663; }
    .pri-3 { color: #bdc1c6; }
    .pri-4 { color: #8ab4f8; }

    /* expanded-row detail panel */
    tr.detail-row td { @apply p-0 border-b; border-color: rgba(255,255,255,0.08); }
    .detail-panel {
      @apply px-5 pt-4 pb-5 pl-12 flex flex-col gap-3.5;
      background: rgba(138,180,248,0.04);
    }
    .detail-grid {
      @apply grid gap-2.5 mb-1;
      grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    }
    .detail-field {
      @apply bg-ink-800/60 border border-white/5 rounded-lg px-3.5 py-2.5;
    }
    .detail-field-lbl {
      @apply text-[10px] font-semibold tracking-wider uppercase text-slate-500 mb-1;
    }
    .detail-field-val { @apply text-[13px] text-slate-200; }
    .detail-section-lbl {
      @apply text-[11px] font-semibold tracking-wider uppercase text-slate-400 mb-1.5;
    }
    .detail-desc { @apply text-sm leading-relaxed text-slate-200 whitespace-pre-wrap; }
    .detail-desc.empty, .no-comments { @apply text-sm text-slate-500 italic; }

    .comments-list { @apply flex flex-col gap-2; }
    .comment {
      @apply bg-ink-800/60 border border-white/5 rounded-lg px-3.5 py-2.5
             border-l-[3px] border-l-violet-400/60;
    }
    .comment-body { @apply text-sm leading-relaxed whitespace-pre-wrap text-slate-200; }
    .comment-meta { @apply text-[11px] text-slate-500 mt-1; }

    tr.group-header td {
      @apply bg-ink-800/80 text-slate-400 text-[11px] font-semibold tracking-wider
             uppercase px-3 py-1.5 border-b border-white/5;
    }
    .empty-state {
      @apply text-center text-slate-500 py-8 text-sm;
    }
    .footer { @apply text-center text-[11px] text-slate-600 mt-8; }

    /* ═══════════════════════════════════════════════════════════════════════
       LIGHT THEME OVERRIDES
       Activated by `<html data-theme="light">`. Targets the most visible
       surfaces (body, nav, cards, tables, badges) and recolors them for a
       Google-Material-style light palette. Accent and warning hues keep
       similar hues but use deeper shades for readability on white.
       ═══════════════════════════════════════════════════════════════════════ */

    html[data-theme="light"] body { background: #f5f6f8; color: #202124; }
    html[data-theme="light"] h1,
    html[data-theme="light"] h2,
    html[data-theme="light"] h3,
    html[data-theme="light"] h4 { color: #202124; }
    html[data-theme="light"] code {
      background: rgba(0,0,0,0.05); color: #1a73e8;
    }

    /* topbar */
    html[data-theme="light"] nav.topbar {
      background: #ffffff;
      border-color: rgba(0,0,0,0.08);
      box-shadow: 0 1px 2px rgba(0,0,0,0.04);
    }
    html[data-theme="light"] nav.topbar .brand { color: #202124; }
    html[data-theme="light"] nav.topbar .brand .accent { color: #1a73e8; }
    html[data-theme="light"] nav.topbar a { color: #5f6368; }
    html[data-theme="light"] nav.topbar a:hover { background: rgba(0,0,0,0.05); color: #202124; }
    html[data-theme="light"] nav.topbar a.active {
      background: rgba(26,115,232,0.10); color: #1a73e8;
      box-shadow: inset 0 -2px 0 #1a73e8;
    }
    html[data-theme="light"] nav.topbar .right { color: #5f6368; }
    html[data-theme="light"] nav.topbar #theme-toggle {
      background: rgba(0,0,0,0.04);
      border-color: rgba(0,0,0,0.10);
      color: #202124;
    }
    html[data-theme="light"] nav.topbar #theme-toggle:hover {
      background: rgba(0,0,0,0.08);
      border-color: rgba(0,0,0,0.18);
    }

    /* stats / cards / surfaces */
    html[data-theme="light"] .stat,
    html[data-theme="light"] .card,
    html[data-theme="light"] .stats,
    html[data-theme="light"] .dash-card,
    html[data-theme="light"] .tbl-card {
      background: #ffffff;
      border-color: rgba(0,0,0,0.08);
      color: #202124;
      box-shadow: 0 1px 3px rgba(0,0,0,0.06), 0 2px 8px rgba(0,0,0,0.04);
    }
    html[data-theme="light"] .stat .value,
    html[data-theme="light"] .dash-card .title { color: #202124; }
    html[data-theme="light"] .stat .label,
    html[data-theme="light"] .stat .sub,
    html[data-theme="light"] .dash-card .desc,
    html[data-theme="light"] .dash-card .cta,
    html[data-theme="light"] .dash-card .metric .k,
    html[data-theme="light"] .tbl-card .rows,
    html[data-theme="light"] .tbl-card .cols { color: #5f6368; }
    html[data-theme="light"] .card h2 { color: #5f6368; }

    /* list / session / job rows */
    html[data-theme="light"] .list .item,
    html[data-theme="light"] .session,
    html[data-theme="light"] .job-row,
    html[data-theme="light"] .msg {
      background: #ffffff;
      border-color: rgba(0,0,0,0.10);
    }
    html[data-theme="light"] .list .item .body,
    html[data-theme="light"] .job-row .row-head .label,
    html[data-theme="light"] .session-head .title { color: #202124; }
    html[data-theme="light"] .list .item .id,
    html[data-theme="light"] .list .item .meta,
    html[data-theme="light"] .session-head .when,
    html[data-theme="light"] .session-head .num,
    html[data-theme="light"] .session-head .caret,
    html[data-theme="light"] .job-row .row-head .id,
    html[data-theme="light"] .job-row .row-head .meta { color: #5f6368; }
    html[data-theme="light"] .job-row .cmd {
      background: rgba(0,0,0,0.04); color: #202124;
    }
    html[data-theme="light"] .session-head:hover { background: rgba(0,0,0,0.03); }

    /* toolbar (filter + group selectors) */
    html[data-theme="light"] .toolbar label { color: #5f6368; }
    html[data-theme="light"] .toolbar select,
    html[data-theme="light"] .toolbar input {
      background: #ffffff;
      border-color: rgba(0,0,0,0.15);
      color: #202124;
    }
    html[data-theme="light"] .toolbar select:focus,
    html[data-theme="light"] .toolbar input:focus {
      border-color: #1a73e8;
      box-shadow: 0 0 0 2px rgba(26,115,232,0.15);
    }

    /* progress / bars */
    html[data-theme="light"] .progress-bar,
    html[data-theme="light"] .bar { background: rgba(0,0,0,0.08); }

    /* todos table */
    html[data-theme="light"] #todo-table {
      background: #ffffff;
      border-color: rgba(0,0,0,0.10);
    }
    html[data-theme="light"] #todo-table th {
      background: #f1f3f4;
      border-color: rgba(0,0,0,0.08);
      color: #5f6368;
    }
    html[data-theme="light"] #todo-table th:hover { color: #1a73e8; }
    html[data-theme="light"] #todo-table th.active .arr { color: #1a73e8; }
    html[data-theme="light"] #todo-table td {
      color: #202124;
      border-color: rgba(0,0,0,0.06);
    }
    html[data-theme="light"] td.col-id,
    html[data-theme="light"] td.col-expand,
    html[data-theme="light"] td.col-date,
    html[data-theme="light"] td.col-tags,
    html[data-theme="light"] td.col-comments,
    html[data-theme="light"] td.col-due,
    html[data-theme="light"] td.col-text .desc-preview { color: #5f6368; }
    html[data-theme="light"] td.col-completed { color: #137333; }
    html[data-theme="light"] td.col-text.done-text { color: #80868b; }

    html[data-theme="light"] tr.data-row:hover td      { background: rgba(0,0,0,0.04); }
    html[data-theme="light"] tr.data-row.expanded td   { background: rgba(26,115,232,0.06); }

    /* column dropdowns */
    html[data-theme="light"] .col-dropdown {
      background: #ffffff;
      border-color: rgba(0,0,0,0.15);
      box-shadow: 0 8px 24px rgba(0,0,0,0.15);
    }
    html[data-theme="light"] .col-dropdown-item { color: #202124; }
    html[data-theme="light"] .col-dropdown-item:hover {
      background: rgba(26,115,232,0.10); color: #1a73e8;
    }
    html[data-theme="light"] .col-dropdown-item.selected { color: #137333; }
    html[data-theme="light"] .col-dropdown-item.clear-item {
      color: #d93025; border-top-color: rgba(0,0,0,0.10);
    }
    html[data-theme="light"] .col-dropdown-item.clear-item:hover {
      background: rgba(217,48,37,0.08);
    }

    /* tags */
    html[data-theme="light"] .tag {
      background: #f1f3f4; border-color: rgba(0,0,0,0.10); color: #3c4043;
    }

    /* due-date tiers (slightly stronger tints on white) */
    html[data-theme="light"] tr.data-row.overdue td   { color: #b3261e !important; }
    html[data-theme="light"] tr.data-row.overdue td.col-due { color: #b3261e !important; }
    html[data-theme="light"] tr.data-row.due-today td { color: #b06000 !important; }
    html[data-theme="light"] tr.data-row.due-today td.col-due { color: #b06000 !important; }
    html[data-theme="light"] tr.data-row.due-soon td.col-due  { color: #8d6e00 !important; }
    html[data-theme="light"] tr.data-row.due-week td.col-due  { color: #1a73e8 !important; }

    /* group header row */
    html[data-theme="light"] tr.group-header td {
      background: #f1f3f4; color: #5f6368;
      border-color: rgba(0,0,0,0.06);
    }

    /* detail panel + comments */
    html[data-theme="light"] .detail-panel  { background: rgba(26,115,232,0.04); }
    html[data-theme="light"] .detail-field  { background: #f8f9fa; border-color: rgba(0,0,0,0.06); }
    html[data-theme="light"] .detail-field-lbl,
    html[data-theme="light"] .detail-section-lbl { color: #5f6368; }
    html[data-theme="light"] .detail-field-val,
    html[data-theme="light"] .detail-desc        { color: #202124; }
    html[data-theme="light"] .detail-desc.empty,
    html[data-theme="light"] .no-comments        { color: #80868b; }
    html[data-theme="light"] .comment            { background: #f8f9fa; border-color: rgba(0,0,0,0.06); }
    html[data-theme="light"] .comment-body       { color: #202124; }
    html[data-theme="light"] .comment-meta       { color: #80868b; }

    /* messages */
    html[data-theme="light"] .msg .head    { color: #5f6368; }
    html[data-theme="light"] .msg .content { color: #202124; }

    /* misc */
    html[data-theme="light"] .empty,
    html[data-theme="light"] .empty-state,
    html[data-theme="light"] footer,
    html[data-theme="light"] .footer       { color: #5f6368; }
    html[data-theme="light"] .donut-center .pct { color: #202124; }
    html[data-theme="light"] .donut-center .lbl { color: #5f6368; }
    html[data-theme="light"] .donut .track      { stroke: rgba(0,0,0,0.08); }
    html[data-theme="light"] .legend .item      { color: #3c4043; }

    /* page lede / titles */
    html[data-theme="light"] .page-lede,
    html[data-theme="light"] .page-header .page-lede { color: #5f6368; }
    html[data-theme="light"] h1.page-title { color: #202124; }
  }
</style>
</head>
<body>
  <nav class="topbar">
    <div class="nav-container">
      <div class="brand"><span class="accent">Omni</span>Launcher</div>
      <div class="links">
        <a href="/dashboard"              data-key="index"        >Overview</a>
        <a href="/dashboard/todos"        data-key="todos"        >Todos</a>
        <a href="/dashboard/conversation" data-key="conversation" >AI Conversation</a>
        <a href="/dashboard/jobs"         data-key="jobs"         >Scheduler</a>
        <a href="/dashboard/tables"       data-key="tables"       >Database</a>
      </div>
      <div class="right">
        <button id="theme-toggle" type="button" aria-label="Toggle theme" title="Toggle light/dark theme">
          <span class="theme-icon">🌙</span>
        </button>
        <span class="pulse"></span>Live · <span id="generated">—</span>
      </div>
    </div>
  </nav>

  <main>
    {{BODY}}
  </main>

  <footer>Auto-refreshes every 5s · served by OmniLauncher live server</footer>

<script>
function esc(s) {
  return String(s ?? '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}
function setActive() {
  const key = "{{ACTIVE}}";
  document.querySelectorAll('nav.topbar a').forEach(a => {
    if (a.dataset.key === key) a.classList.add('active');
  });
}
function stampGenerated(d) {
  const g = document.getElementById('generated');
  if (g && d && d.generated_at) g.textContent = d.generated_at;
}

// ── theme toggle (light/dark) — persisted in localStorage ──────────────────
function applyTheme(t) {
  document.documentElement.setAttribute('data-theme', t);
  const btn = document.querySelector('#theme-toggle .theme-icon');
  if (btn) btn.textContent = (t === 'light') ? '☀' : '🌙';
}
function initThemeToggle() {
  const stored = localStorage.getItem('omnilauncher.dashboard.theme') || 'dark';
  applyTheme(stored);
  const btn = document.getElementById('theme-toggle');
  if (!btn) return;
  btn.addEventListener('click', () => {
    const cur = document.documentElement.getAttribute('data-theme') || 'dark';
    const next = cur === 'dark' ? 'light' : 'dark';
    localStorage.setItem('omnilauncher.dashboard.theme', next);
    applyTheme(next);
  });
}
initThemeToggle();
setActive();
{{SCRIPT}}
</script>
</body>
</html>
"##;
