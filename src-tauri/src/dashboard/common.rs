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
          ink: {
            900: '#0b0f1c',
            800: '#10162a',
            700: '#161d36',
            600: '#1e2745',
            500: '#2a345a',
          },
        },
        boxShadow: {
          card: '0 10px 30px -12px rgba(0,0,0,0.5), 0 2px 6px rgba(0,0,0,0.25)',
          glow: '0 0 0 1px rgba(56,189,248,0.15), 0 10px 40px -10px rgba(56,189,248,0.25)',
        },
        keyframes: {
          pulseDot: {
            '0%':   { boxShadow: '0 0 0 0 rgba(16,185,129,0.6)' },
            '70%':  { boxShadow: '0 0 0 10px rgba(16,185,129,0)' },
            '100%': { boxShadow: '0 0 0 0 rgba(16,185,129,0)' },
          },
        },
        animation: { pulseDot: 'pulseDot 1.8s infinite' },
      },
    },
  }
</script>
<style type="text/tailwindcss">
  /* ---------- compat: keep legacy var(--*) usages working with the new palette ---------- */
  :root {
    --bg:        #0b0f1c;
    --bg-2:      #10162a;
    --surface:   #161d36;
    --surface-2: #1e2745;
    --border:    rgba(255,255,255,0.06);
    --text:      #e2e8f0;
    --sub:       #94a3b8;
    --accent:    #38bdf8;
    --green:     #34d399;
    --yellow:    #fbbf24;
    --red:       #fb7185;
    --purple:    #a78bfa;
    --cyan:      #22d3ee;
  }

  @layer base {
    * { box-sizing: border-box; }
    html, body { @apply m-0 p-0 min-h-screen text-slate-200 antialiased font-sans; }
    body {
      background:
        radial-gradient(1200px 600px at 90% -10%, rgba(56,189,248,0.10), transparent 60%),
        radial-gradient(900px 500px at -10% 20%, rgba(139,92,246,0.10), transparent 60%),
        #0b0f1c;
    }
    h1, h2, h3, h4 { @apply text-slate-100; }
    a { @apply transition-colors; }
    code { @apply font-mono text-[12px] px-1.5 py-0.5 rounded bg-white/5 text-sky-300; }
  }

  @layer components {
    /* ---------- layout shell ---------- */
    nav.topbar { @apply sticky top-0 z-20 px-4 sm:px-8 py-3 border-b border-white/5
                       bg-ink-900/70 backdrop-blur-xl flex items-center gap-5 flex-wrap; }
    .nav-container { @apply max-w-[1400px] w-full mx-auto flex items-center gap-5 flex-wrap; }
    nav.topbar .brand { @apply font-bold text-[15px] tracking-tight text-slate-100; }
    nav.topbar .brand .accent {
      @apply bg-clip-text text-transparent bg-gradient-to-r from-sky-400 to-indigo-400;
    }
    nav.topbar .links { @apply flex gap-1 flex-wrap; }
    nav.topbar a {
      @apply text-slate-400 no-underline px-3 py-1.5 rounded-lg text-[13px] font-medium
             hover:bg-white/5 hover:text-slate-100;
    }
    nav.topbar a.active {
      @apply bg-white/5 text-slate-100 shadow-[inset_0_-2px_0_theme(colors.sky.400)];
    }
    nav.topbar .right { @apply ml-auto text-slate-400 text-xs flex items-center; }
    nav.topbar .pulse {
      @apply inline-block w-2 h-2 rounded-full bg-emerald-400 mr-2 align-middle animate-pulseDot;
    }

    main { @apply max-w-[1400px] mx-auto px-4 sm:px-8 pt-6 pb-16; }
    h1.page-title { @apply text-2xl font-bold mb-6 text-slate-100 tracking-tight; }

    /* ---------- stat cards ---------- */
    .grid-stats { @apply grid gap-3.5 mb-6;
                  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); }
    .stat {
      @apply rounded-2xl border border-white/5 p-4 shadow-card
             bg-gradient-to-b from-ink-700/80 to-ink-800/80 backdrop-blur-sm
             transition hover:border-white/10 hover:-translate-y-0.5;
    }
    .stat .label { @apply text-slate-500 text-[11px] uppercase tracking-widest font-semibold mb-1.5; }
    .stat .value { @apply text-3xl font-bold leading-tight text-slate-100; }
    .stat .sub   { @apply text-slate-500 text-xs mt-1; }
    .stat.green  .value { @apply text-emerald-400; }
    .stat.yellow .value { @apply text-amber-400; }
    .stat.red    .value { @apply text-rose-400; }
    .stat.accent .value { @apply text-sky-400; }
    .stat.purple .value { @apply text-violet-400; }
    .stat.cyan   .value { @apply text-cyan-400; }

    /* ---------- main grid + cards ---------- */
    .grid-main { @apply grid gap-4;
                 grid-template-columns: repeat(auto-fit, minmax(360px, 1fr)); }
    .card {
      @apply rounded-2xl border border-white/5 bg-ink-700/60 backdrop-blur-sm
             p-5 shadow-card;
    }
    .card h2 {
      @apply text-[12px] uppercase tracking-widest text-slate-400 mb-3 font-semibold;
    }

    /* ---------- progress bars ---------- */
    .bar-row { @apply flex items-center gap-2.5 mb-2 text-[13px]; }
    .bar-row .label { @apply min-w-[110px] text-slate-200; }
    .bar-row .count { @apply min-w-[36px] text-right text-slate-500 tabular-nums; }
    .bar { @apply flex-1 h-2 bg-white/5 rounded-full overflow-hidden; }
    .bar > span { @apply block h-full rounded-full transition-all duration-500; }
    .bar.todo        > span { @apply bg-gradient-to-r from-slate-500 to-slate-400; }
    .bar.in_progress > span { @apply bg-gradient-to-r from-sky-500 to-sky-300; }
    .bar.blocked     > span { @apply bg-gradient-to-r from-rose-500 to-rose-300; }
    .bar.done        > span { @apply bg-gradient-to-r from-emerald-500 to-emerald-300; }

    /* ---------- charts ---------- */
    .chart { @apply flex items-end gap-1 h-24 pt-1.5; }
    .chart .col { @apply flex-1 flex flex-col items-stretch justify-end min-w-[8px]; }
    .chart .col > span {
      @apply block rounded-t min-h-[2px] transition-all duration-500
             bg-gradient-to-b from-sky-400 to-sky-700;
    }
    .chart .col.b > span { @apply bg-gradient-to-b from-emerald-400 to-emerald-700; }
    .chart-axis { @apply flex gap-1 mt-1.5 text-slate-500 text-[10px]; }
    .chart-axis .col { @apply flex-1 text-center min-w-[8px] overflow-hidden whitespace-nowrap; }

    /* ---------- lists ---------- */
    .list { @apply flex flex-col gap-2; }
    .list .item {
      @apply flex items-center gap-2.5 px-3 py-2 rounded-xl
             bg-ink-800/60 border border-white/5 text-[13px]
             hover:border-white/10 transition;
    }
    .list .item .id { @apply text-slate-500 text-[11px] font-mono min-w-[36px]; }
    .list .item .body { @apply flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-slate-200; }
    .list .item .meta { @apply text-slate-500 text-[11px]; }

    /* ---------- badges (status) ---------- */
    .badge {
      @apply inline-block px-2 py-0.5 rounded-full text-[10px] font-semibold
             tracking-wider uppercase border border-transparent;
    }
    .badge.todo        { @apply bg-slate-700/60 text-slate-300; }
    .badge.in_progress { @apply bg-sky-900/60 text-sky-300 border-sky-700/50; }
    .badge.blocked     { @apply bg-rose-900/40 text-rose-300 border-rose-700/50; }
    .badge.done        { @apply bg-emerald-900/40 text-emerald-300 border-emerald-700/50; }

    /* ---------- priority pills ---------- */
    .pri { @apply text-[11px] font-semibold px-1.5 py-px rounded border; }
    .pri-1 { @apply text-rose-300    bg-rose-900/40    border-rose-800/60; }
    .pri-2 { @apply text-amber-300   bg-amber-900/30   border-amber-800/60; }
    .pri-3 { @apply text-slate-300   bg-slate-700/40   border-slate-600/50; }
    .pri-4 { @apply text-sky-300     bg-sky-900/30     border-sky-800/60; }
    .pri-5 { @apply text-emerald-300 bg-emerald-900/30 border-emerald-800/60; }

    /* ---------- tags ---------- */
    .tag-cloud { @apply flex flex-wrap gap-1.5; }
    .tag {
      @apply inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full
             bg-ink-800/60 border border-white/5 text-[11px] text-slate-300;
    }
    .tag .c { @apply text-sky-400 font-bold; }

    /* ---------- messages ---------- */
    .msg {
      @apply flex flex-col gap-1 px-3 py-2.5 rounded-xl
             bg-ink-800/60 border border-white/5 text-[12.5px];
    }
    .msg .head { @apply flex justify-between text-slate-500 text-[11px]; }
    .msg.user      { @apply border-l-[3px] border-l-sky-400; }
    .msg.assistant { @apply border-l-[3px] border-l-violet-400; }
    .msg .content {
      @apply text-slate-200 leading-relaxed overflow-hidden whitespace-pre-wrap;
      display: -webkit-box; -webkit-line-clamp: 6; -webkit-box-orient: vertical;
    }

    /* ---------- misc ---------- */
    .empty { @apply text-slate-500 text-xs py-2.5 px-1; }

    .donut-wrap { @apply flex items-center gap-4; }
    .donut { @apply w-28 h-28 flex-shrink-0 -rotate-90; }
    .donut circle { fill: none; stroke-width: 14; }
    .donut .track { @apply stroke-white/5; }
    .donut-center { @apply flex flex-col items-start; }
    .donut-center .pct { @apply text-3xl font-bold text-slate-100; }
    .donut-center .lbl { @apply text-slate-500 text-[11px] uppercase tracking-widest; }
    .legend { @apply flex flex-col gap-1.5 text-xs; }
    .legend .item { @apply flex items-center gap-2 text-slate-400; }
    .legend .item .dot { @apply w-2.5 h-2.5 rounded-full; }

    footer { @apply my-8 mx-8 text-center text-slate-500 text-[11px]; }

    /* ---------- index page: dashboard cards grid ---------- */
    .dash-grid { @apply grid gap-4;
                 grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); }
    .dash-card {
      @apply relative flex flex-col gap-3 p-6 rounded-2xl no-underline
             border border-white/5 bg-gradient-to-b from-ink-700/80 to-ink-800/80
             text-slate-200 shadow-card overflow-hidden
             transition hover:-translate-y-1 hover:border-white/15
             hover:shadow-[0_18px_40px_-12px_rgba(0,0,0,0.5)];
    }
    .dash-card::before {
      content: ""; position: absolute; inset: 0;
      background: radial-gradient(400px 200px at 100% 0%, var(--card-glow, rgba(56,189,248,0.18)), transparent 70%);
      pointer-events: none;
    }
    .dash-card > * { @apply relative; }
    .dash-card .head { @apply flex items-center gap-3; }
    .dash-card .icon {
      @apply w-10 h-10 flex-shrink-0 rounded-xl inline-flex items-center justify-center
             text-xl font-bold text-white shadow-lg;
      background: var(--card-accent, theme(colors.sky.500));
    }
    .dash-card .title { @apply text-base font-bold text-slate-100; }
    .dash-card .desc { @apply text-slate-400 text-[12.5px] leading-relaxed; }
    .dash-card .metrics { @apply flex gap-5 mt-1; }
    .dash-card .metric { @apply flex flex-col gap-0.5; }
    .dash-card .metric .v {
      @apply text-2xl font-bold tabular-nums;
      color: var(--card-accent, theme(colors.sky.400));
    }
    .dash-card .metric .k {
      @apply text-[10.5px] text-slate-500 uppercase tracking-widest;
    }
    .dash-card .cta {
      @apply mt-auto pt-1.5 text-xs text-slate-500 flex items-center gap-1.5;
    }
    .dash-card .cta .arr { @apply transition-transform; }
    .dash-card:hover .cta { @apply text-slate-200; }
    .dash-card:hover .cta .arr { @apply translate-x-1; }

    /* ---------- conversation page: collapsible session ---------- */
    .session {
      @apply rounded-xl border border-white/5 bg-ink-800/60 overflow-hidden
             transition hover:border-white/10;
    }
    .session.open { @apply border-sky-500/40; }
    .session-head {
      @apply flex items-center gap-2.5 px-4 py-3 cursor-pointer select-none
             hover:bg-white/[0.03];
    }
    .session-head .caret { @apply text-slate-500 transition-transform; }
    .session.open .session-head .caret { @apply rotate-90 text-sky-400; }
    .session-head .num { @apply font-mono text-[11px] text-slate-500 min-w-[28px]; }
    .session-head .title {
      @apply flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-semibold text-[13.5px];
    }
    .session-head .pill {
      @apply text-[10.5px] px-2 py-0.5 rounded-full bg-white/5 text-slate-400
             border border-white/5;
    }
    .session-head .pill.u { @apply text-sky-300 border-sky-400/30; }
    .session-head .pill.a { @apply text-violet-300 border-violet-400/30; }
    .session-head .when { @apply text-slate-500 text-[11px]; }
    .session-body {
      @apply px-4 pt-1 pb-3.5 flex flex-col gap-2 border-t border-white/5;
    }

    /* ---------- jobs page: row cards ---------- */
    .job-row {
      @apply px-4 py-3 rounded-xl border border-white/5 bg-ink-800/60
             flex flex-col gap-2 transition hover:border-white/10;
    }
    .job-row .row-head { @apply flex items-center gap-2.5 flex-wrap; }
    .job-row .row-head .id { @apply text-slate-500 font-mono text-[11px]; }
    .job-row .row-head .label { @apply font-semibold text-slate-100; }
    .job-row .row-head .meta { @apply ml-auto text-slate-500 text-[11px]; }
    .job-row .row-grid {
      @apply grid gap-2 text-xs text-slate-300;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    }
    .job-row .row-grid .k { @apply text-slate-500 mr-1; }
    .job-row .cmd {
      @apply text-xs px-2.5 py-2 rounded-md bg-white/5 text-slate-200
             font-mono break-all;
    }

    /* ---------- tables page: table cards ---------- */
    .tbl-grid {
      @apply grid gap-3.5;
      grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    }
    .tbl-card {
      @apply rounded-2xl border border-white/5 bg-ink-700/60 backdrop-blur-sm
             p-4 shadow-card flex flex-col gap-2.5
             transition hover:border-white/10;
    }
    .tbl-card .head { @apply flex items-baseline justify-between gap-2; }
    .tbl-card .name { @apply font-mono font-semibold text-sky-300 text-sm; }
    .tbl-card .rows { @apply text-slate-500 text-[11px] tabular-nums; }
    .tbl-card .cols { @apply text-slate-500 text-[10.5px] leading-snug break-words; }
    .tbl-card .samples { @apply flex flex-col gap-1; }
    .tbl-card .sample {
      @apply px-2 py-1.5 rounded-lg bg-white/5 border border-white/5
             flex flex-col gap-0.5;
    }
    .tbl-card .sample .kv { @apply flex gap-1.5 text-[11px] leading-snug; }
    .tbl-card .sample .kv .k {
      @apply text-slate-500 min-w-[80px] flex-shrink-0;
    }
    .tbl-card .sample .kv .v {
      @apply text-slate-200 overflow-hidden text-ellipsis whitespace-nowrap;
    }
    .page-lede { @apply text-slate-400 -mt-3 mb-6 text-[13px]; }

    /* ─────────────────────────────────────────────────────────────────────
       todos page (HTML produced by plugins/todo.rs `generate_html`).
       Class names below match what that template renders so it picks up
       the unified dashboard look without changes inside the plugin.
       ───────────────────────────────────────────────────────────────────── */

    /* stats strip at the top */
    .stats {
      @apply flex items-center gap-7 flex-wrap mb-6 px-5 py-4
             rounded-2xl border border-white/5 bg-ink-700/60 shadow-card;
    }
    .stats .stat { @apply bg-transparent border-0 shadow-none p-0 text-center; }
    .stats .stat .num { @apply text-2xl font-bold text-sky-300 tabular-nums; }
    .stats .stat .lbl { @apply text-[11px] text-slate-500 mt-0.5 uppercase tracking-wider; }
    .progress-wrap { @apply flex-1 min-w-[140px]; }
    .progress-lbl  { @apply text-[11px] text-slate-500 mb-1.5; }
    .progress-bar  { @apply h-2 bg-white/5 rounded-full overflow-hidden; }
    .progress-fill {
      @apply h-full rounded-full bg-gradient-to-r from-emerald-400 to-emerald-300 transition-all duration-500;
    }

    /* search + group toolbar */
    .toolbar { @apply flex items-center gap-2.5 mb-4 flex-wrap; }
    .toolbar label { @apply text-xs text-slate-500; }
    .toolbar select,
    .toolbar input {
      @apply bg-ink-800/80 border border-white/10 text-slate-200
             rounded-lg px-3 py-1.5 text-[13px] outline-none cursor-pointer
             focus:border-sky-400 focus:ring-2 focus:ring-sky-400/20 transition;
    }
    .toolbar input { @apply flex-1 min-w-[160px] cursor-text; }

    /* todos table */
    #todo-table {
      @apply w-full border-collapse rounded-2xl overflow-hidden
             border border-white/5 bg-ink-700/40 shadow-card;
    }
    #todo-table th {
      @apply relative px-3 py-2.5 text-[11px] font-semibold tracking-wider uppercase
             text-slate-400 text-left cursor-pointer select-none whitespace-nowrap
             bg-ink-800/80 border-b border-white/5 transition;
    }
    #todo-table th:hover { @apply text-sky-300; }
    #todo-table th .arr { @apply ml-1 opacity-40 text-[10px]; }
    #todo-table th.active .arr { @apply opacity-100 text-sky-300; }
    #todo-table th .col-filter-badge {
      @apply inline-block ml-1.5 px-1.5 py-px rounded-full bg-sky-500 text-ink-900
             text-[9px] font-bold normal-case tracking-normal align-middle;
    }
    .col-dropdown {
      @apply absolute top-full left-0 z-50 min-w-[160px] hidden
             bg-ink-800 border border-white/10 rounded-lg shadow-2xl overflow-hidden;
    }
    .col-dropdown.open { @apply block; }
    .col-dropdown-item {
      @apply px-3.5 py-2 text-xs text-slate-200 cursor-pointer whitespace-nowrap
             normal-case tracking-normal font-normal hover:bg-sky-500/10 hover:text-sky-300;
    }
    .col-dropdown-item.selected { @apply text-emerald-300 font-semibold; }
    .col-dropdown-item.clear-item {
      @apply text-rose-300 border-t border-white/10 mt-0.5 hover:bg-rose-500/10;
    }

    tr.data-row { @apply cursor-pointer transition-colors; }
    tr.data-row:hover td { @apply bg-white/[0.03]; }
    tr.data-row.expanded td { @apply bg-sky-500/[0.06] border-b-0; }
    tr.data-row.overdue td.col-due { @apply text-rose-400 font-semibold; }
    #todo-table td {
      @apply px-3 py-2.5 text-[13.5px] align-middle border-b border-white/5 text-slate-200;
    }
    td.col-expand { @apply w-6 text-slate-500 text-xs pr-0; }
    td.col-id { @apply text-slate-500 w-12 text-xs tabular-nums; }
    td.col-text { @apply leading-snug min-w-[180px]; }
    td.col-text .desc-preview {
      @apply text-[11px] text-slate-500 mt-0.5 truncate max-w-[320px];
    }
    td.col-text.done-text { @apply text-slate-500 line-through; }
    td.col-pri { @apply w-[100px] whitespace-nowrap; }
    td.col-status { @apply w-[100px] whitespace-nowrap; }
    td.col-due { @apply w-[98px] text-xs text-slate-500 whitespace-nowrap; }
    td.col-tags { @apply w-[130px] text-xs; }
    td.col-date { @apply text-slate-500 text-xs w-[94px] whitespace-nowrap; }
    td.col-completed { @apply text-emerald-300 text-xs w-[94px] whitespace-nowrap; }
    td.col-comments { @apply w-16 text-xs text-slate-500 text-center; }

    /* todo badges (hyphenated variants used by the todos page) */
    .badge-todo {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full
             bg-slate-700/50 text-slate-300 border border-slate-600/40;
    }
    .badge-in-progress {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full
             bg-sky-500/10 text-sky-300 border border-sky-500/30;
    }
    .badge-blocked {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full
             bg-amber-500/10 text-amber-300 border border-amber-500/30;
    }
    .badge-done {
      @apply inline-flex items-center gap-1 text-[11px] font-semibold px-2 py-0.5 rounded-full
             bg-emerald-500/10 text-emerald-300 border border-emerald-500/30;
    }

    /* todos page priority colors (overrides .pri-1..5 from above with stronger color) */
    .pri-1 { @apply text-rose-400; }
    .pri-2 { @apply text-amber-300; }
    .pri-3 { @apply text-slate-300; }
    .pri-4 { @apply text-sky-300; }

    /* expanded-row detail panel */
    tr.detail-row td { @apply p-0 border-b border-white/5; }
    .detail-panel {
      @apply px-5 pt-4 pb-5 pl-12 bg-sky-500/[0.04] flex flex-col gap-3.5;
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
      <div class="right"><span class="pulse"></span>Live · <span id="generated">—</span></div>
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
setActive();
{{SCRIPT}}
</script>
</body>
</html>
"##;
