<?php
if (isset($_GET['q']) && $_GET['q'] === 'info') {
    phpinfo();
    exit;
}

function es_h($val)
{
    return htmlspecialchars((string) $val, ENT_QUOTES, 'UTF-8');
}

function es_sanitize_label($name)
{
    $lower = strtolower((string) $name);
    $cleaned = preg_replace('/[^a-z0-9.-]/', '-', $lower);
    return trim((string) $cleaned, '-');
}

$rootDir = dirname(__DIR__);
$cfgPath = $rootDir . DIRECTORY_SEPARATOR . 'envstack.json';
$cfg = [];
if (is_file($cfgPath)) {
    $raw = @file_get_contents($cfgPath);
    if (is_string($raw)) {
        $decoded = json_decode($raw, true);
        if (is_array($decoded)) {
            $cfg = $decoded;
        }
    }
}

$defaultTheme = (isset($cfg['theme']) && $cfg['theme'] === 'light') ? 'light' : 'dark';
$tld = isset($cfg['tld']) ? ltrim(trim((string) $cfg['tld']), '.') : 'test';
if ($tld === '') {
    $tld = 'test';
}
$autoSsl = isset($cfg['auto_ssl']) ? (bool) $cfg['auto_ssl'] : true;
$ports = (isset($cfg['ports']) && is_array($cfg['ports'])) ? $cfg['ports'] : [];

$nginxHttp = isset($ports['nginx_http']) ? (int) $ports['nginx_http'] : 8080;
$nginxHttps = isset($ports['nginx_https']) ? (int) $ports['nginx_https'] : 8443;
$apacheHttp = isset($ports['apache_http']) ? (int) $ports['apache_http'] : 80;
$apacheHttps = isset($ports['apache_https']) ? (int) $ports['apache_https'] : 443;
$mysqlPort = isset($ports['mysql']) ? (int) $ports['mysql'] : 3306;
$mailpitPort = isset($ports['mailpit_http']) ? (int) $ports['mailpit_http'] : 8025;

$serverPort = isset($_SERVER['SERVER_PORT']) ? (int) $_SERVER['SERVER_PORT'] : $nginxHttp;
$serverSoftware = isset($_SERVER['SERVER_SOFTWARE']) ? (string) $_SERVER['SERVER_SOFTWARE'] : 'Local Web Server';
$httpHost = isset($_SERVER['HTTP_HOST']) ? (string) $_SERVER['HTTP_HOST'] : ('localhost' . ($serverPort === 80 ? '' : ':' . $serverPort));

if ($serverPort === $apacheHttp || $serverPort === $apacheHttps || stripos($serverSoftware, 'apache') !== false) {
    $activeHttpPort = $apacheHttp;
    $activeHttpsPort = $apacheHttps;
} else {
    $activeHttpPort = $nginxHttp;
    $activeHttpsPort = $nginxHttps;
}

$sslCertsDir = $rootDir . DIRECTORY_SEPARATOR . 'etc' . DIRECTORY_SEPARATOR . 'ssl' . DIRECTORY_SEPARATOR . 'certs';
$wwwDir = __DIR__;
$projects = [];
$entries = @scandir($wwwDir);
if (is_array($entries)) {
    foreach ($entries as $entry) {
        if ($entry === '.' || $entry === '..' || strpos($entry, '.') === 0) {
            continue;
        }
        $fullPath = $wwwDir . DIRECTORY_SEPARATOR . $entry;
        if (!is_dir($fullPath)) {
            continue;
        }
        if (is_file($fullPath . DIRECTORY_SEPARATOR . 'src-tauri' . DIRECTORY_SEPARATOR . 'tauri.conf.json')) {
            continue;
        }

        $label = es_sanitize_label($entry);
        if ($label === '') {
            continue;
        }

        if (is_file($fullPath . DIRECTORY_SEPARATOR . 'artisan')) {
            $framework = 'Laravel';
        } elseif (
            is_file($fullPath . DIRECTORY_SEPARATOR . 'wp-config.php') ||
            is_file($fullPath . DIRECTORY_SEPARATOR . 'wp-config-sample.php')
        ) {
            $framework = 'WordPress';
        } elseif (is_file($fullPath . DIRECTORY_SEPARATOR . 'composer.json')) {
            $framework = 'PHP / Composer';
        } elseif (
            is_file($fullPath . DIRECTORY_SEPARATOR . 'index.php') ||
            is_file($fullPath . DIRECTORY_SEPARATOR . 'public' . DIRECTORY_SEPARATOR . 'index.php')
        ) {
            $framework = 'PHP';
        } else {
            $framework = 'Static';
        }

        $docSuffix = '';
        if (is_dir($fullPath . DIRECTORY_SEPARATOR . 'public')) {
            $docSuffix = '/public';
        } elseif (is_dir($fullPath . DIRECTORY_SEPARATOR . 'web')) {
            $docSuffix = '/web';
        }

        $domain = $label . '.' . $tld;
        $hasCert = is_file($sslCertsDir . DIRECTORY_SEPARATOR . $domain . '.crt')
            && is_file($sslCertsDir . DIRECTORY_SEPARATOR . $domain . '.key');

        $httpUrl = 'http://' . $domain . ($activeHttpPort === 80 ? '' : ':' . $activeHttpPort);
        $httpsUrl = 'https://' . $domain . ($activeHttpsPort === 443 ? '' : ':' . $activeHttpsPort);
        $primaryUrl = ($autoSsl && $hasCert) ? $httpsUrl : $httpUrl;

        $projects[] = [
            'name' => $entry,
            'domain' => $domain,
            'framework' => $framework,
            'doc_suffix' => $docSuffix,
            'has_ssl' => $hasCert,
            'primary_url' => $primaryUrl,
            'sub_url' => './' . rawurlencode($entry) . '/',
        ];
    }
}

usort($projects, function ($a, $b) {
    return strcasecmp($a['name'], $b['name']);
});

$phpBits = PHP_INT_SIZE === 8 ? 'x64' : 'x86';
$loadedExtCount = count(get_loaded_extensions());
$docRootNormalized = str_replace('\\', '/', $wwwDir);
$mailpitUrl = 'http://127.0.0.1:' . $mailpitPort;

$keyExtensions = [
    'pdo_mysql',
    'mysqli',
    'curl',
    'mbstring',
    'openssl',
    'gd',
    'intl',
    'zip',
    'fileinfo',
    'pdo_sqlite',
    'exif',
    'sockets',
];
?>
<!DOCTYPE html>
<html lang="en" data-theme="<?= es_h($defaultTheme) ?>">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>EnvStackUI — Local Development Environment</title>
  <style>
    :root,
    [data-theme="dark"] {
      --bg-app: #21252b;
      --bg-sidebar: #1d2026;
      --bg-surface: #282c33;
      --bg-elevated: #2f343e;
      --bg-hover: #353b47;
      --bg-input: #1f2329;
      --border: #3b414d;
      --border-subtle: #2e333d;
      --text-primary: #dce0e5;
      --text-muted: #929aa8;
      --text-dim: #636b78;
      --accent: #74ade8;
      --accent-fg: #181c22;
      --accent-soft: rgba(116, 173, 232, 0.14);
      --green: #a1c181;
      --green-soft: rgba(161, 193, 129, 0.14);
      --amber: #dec184;
      --amber-soft: rgba(222, 193, 132, 0.14);
      color-scheme: dark;
    }

    [data-theme="light"] {
      --bg-app: #f0f1f3;
      --bg-sidebar: #e7e8eb;
      --bg-surface: #fafafa;
      --bg-elevated: #ffffff;
      --bg-hover: #eceef1;
      --bg-input: #ffffff;
      --border: #d5d7dc;
      --border-subtle: #e3e5e9;
      --text-primary: #242529;
      --text-muted: #585d68;
      --text-dim: #8a909b;
      --accent: #4c6fd8;
      --accent-fg: #ffffff;
      --accent-soft: rgba(76, 111, 216, 0.12);
      --green: #458536;
      --green-soft: rgba(69, 133, 54, 0.12);
      --amber: #ad7218;
      --amber-soft: rgba(173, 114, 24, 0.12);
      color-scheme: light;
    }

    * { box-sizing: border-box; }

    html, body {
      margin: 0;
      padding: 0;
      min-height: 100%;
      background: var(--bg-app);
      color: var(--text-primary);
      font-family: "Inter", "Segoe UI", -apple-system, BlinkMacSystemFont, sans-serif;
      font-size: 13px;
      line-height: 1.45;
      -webkit-font-smoothing: antialiased;
    }

    a { color: inherit; text-decoration: none; }

    .mono {
      font-family: "JetBrains Mono", "Cascadia Code", "Fira Code", Consolas, monospace;
    }

    .topbar {
      background: var(--bg-sidebar);
      border-bottom: 1px solid var(--border);
      position: sticky;
      top: 0;
      z-index: 20;
    }

    .topbar-inner {
      max-width: 1120px;
      margin: 0 auto;
      padding: 10px 20px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      flex-wrap: wrap;
    }

    .brand {
      display: flex;
      align-items: center;
      gap: 10px;
    }

    .brand-title {
      font-weight: 600;
      font-size: 14px;
      letter-spacing: -0.01em;
    }

    .pill {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      font-size: 11px;
      padding: 2px 7px;
      border-radius: 4px;
      border: 1px solid var(--border);
      background: var(--bg-surface);
      color: var(--text-muted);
    }

    .pill-green {
      background: var(--green-soft);
      border-color: transparent;
      color: var(--green);
    }

    .actions {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-wrap: wrap;
    }

    .btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 5px 10px;
      border-radius: 4px;
      border: 1px solid var(--border);
      background: var(--bg-elevated);
      color: var(--text-primary);
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      transition: background 0.12s ease, border-color 0.12s ease;
    }

    .btn:hover {
      background: var(--bg-hover);
      border-color: var(--accent);
    }

    .btn-primary {
      background: var(--accent);
      border-color: var(--accent);
      color: var(--accent-fg);
    }

    .btn-primary:hover {
      filter: brightness(1.06);
      border-color: var(--accent);
      background: var(--accent);
    }

    .container {
      max-width: 1120px;
      margin: 0 auto;
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 18px;
    }

    .kpi-grid {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 10px;
    }

    @media (max-width: 900px) {
      .kpi-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    }

    @media (max-width: 540px) {
      .kpi-grid { grid-template-columns: 1fr; }
    }

    .panel {
      background: var(--bg-surface);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 13px 14px;
    }

    .kpi-label {
      font-size: 11px;
      color: var(--text-muted);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    .kpi-value {
      margin-top: 6px;
      font-size: 15px;
      font-weight: 600;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .kpi-sub {
      margin-top: 3px;
      font-size: 11px;
      color: var(--text-dim);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .section-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      flex-wrap: wrap;
      margin-bottom: 10px;
    }

    .section-title {
      font-size: 13px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .input {
      background: var(--bg-input);
      border: 1px solid var(--border);
      color: var(--text-primary);
      border-radius: 4px;
      padding: 5px 10px;
      font-size: 12px;
      outline: none;
      min-width: 220px;
    }

    .input:focus { border-color: var(--accent); }
    .input::placeholder { color: var(--text-dim); }

    .projects-grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 10px;
    }

    @media (max-width: 760px) {
      .projects-grid { grid-template-columns: 1fr; }
    }

    .project-card {
      background: var(--bg-elevated);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 12px 14px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      transition: border-color 0.12s ease;
    }

    .project-card:hover { border-color: var(--accent); }

    .project-meta { min-width: 0; flex: 1; }

    .project-name {
      font-size: 13px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 6px;
      flex-wrap: wrap;
    }

    .tag {
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 4px;
      background: var(--bg-surface);
      border: 1px solid var(--border-subtle);
      color: var(--text-muted);
    }

    .tag-accent {
      background: var(--accent-soft);
      border-color: transparent;
      color: var(--accent);
    }

    .project-domain {
      margin-top: 4px;
      font-size: 11px;
      color: var(--accent);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .project-actions {
      display: flex;
      align-items: center;
      gap: 6px;
      flex-shrink: 0;
    }

    .ext-grid {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }

    .ext-pill {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 3px 8px;
      border-radius: 4px;
      border: 1px solid var(--border);
      background: var(--bg-elevated);
      font-size: 11px;
      color: var(--text-muted);
    }

    .dot {
      width: 6px;
      height: 6px;
      border-radius: 999px;
      background: var(--text-dim);
    }

    .dot-ok { background: var(--green); }

    .empty-box {
      padding: 28px 16px;
      text-align: center;
      color: var(--text-muted);
      border: 1px dashed var(--border);
      border-radius: 6px;
      background: var(--bg-input);
    }

    .footer {
      border-top: 1px solid var(--border-subtle);
      padding-top: 12px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      flex-wrap: wrap;
      gap: 8px;
      font-size: 11px;
      color: var(--text-dim);
    }
  </style>
</head>
<body>
  <header class="topbar">
    <div class="topbar-inner">
      <div class="brand">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <rect x="2" y="2" width="20" height="20" rx="5" fill="var(--accent-soft)" stroke="var(--accent)" stroke-width="1.5"/>
          <path d="M7 9l4 3-4 3M13 15h4" stroke="var(--accent)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
        <span class="brand-title">EnvStackUI</span>
        <span class="pill pill-green mono">Local Dev Server</span>
      </div>

      <div class="actions">
        <a href="?q=info" class="btn mono">phpinfo()</a>
        <a href="/__adminer/" class="btn mono">Adminer DB</a>
        <a href="<?= es_h($mailpitUrl) ?>" target="_blank" rel="noopener" class="btn mono">Mailpit UI</a>
        <button type="button" id="themeToggle" class="btn mono" onclick="toggleTheme()">Theme</button>
      </div>
    </div>
  </header>

  <main class="container">
    <section class="kpi-grid">
      <div class="panel">
        <div class="kpi-label">
          <span>Web Server</span>
          <span class="dot dot-ok"></span>
        </div>
        <div class="kpi-value mono" title="<?= es_h($serverSoftware) ?>"><?= es_h($serverSoftware) ?></div>
        <div class="kpi-sub mono"><?= es_h($httpHost) ?></div>
      </div>

      <div class="panel">
        <div class="kpi-label">
          <span>PHP Runtime</span>
          <span class="pill tag-accent mono"><?= es_h($phpBits) ?></span>
        </div>
        <div class="kpi-value mono">PHP <?= es_h(PHP_VERSION) ?></div>
        <div class="kpi-sub mono"><?= es_h(PHP_SAPI) ?> &middot; <?= $loadedExtCount ?> extensions</div>
      </div>

      <div class="panel">
        <div class="kpi-label">
          <span>Document Root</span>
          <span class="mono" style="color: var(--accent);"><?= count($projects) ?> sites</span>
        </div>
        <div class="kpi-value mono" title="<?= es_h($docRootNormalized) ?>"><?= es_h($docRootNormalized) ?></div>
        <div class="kpi-sub mono">Auto-VHost: *.<?= es_h($tld) ?></div>
      </div>

      <div class="panel">
        <div class="kpi-label">
          <span>Database &amp; Mail</span>
          <span class="mono" style="color: var(--amber);">Local Ports</span>
        </div>
        <div class="kpi-value mono">MySQL :<?= $mysqlPort ?></div>
        <div class="kpi-sub mono">Mailpit :<?= $mailpitPort ?></div>
      </div>
    </section>

    <section class="panel">
      <div class="section-head">
        <div class="section-title">
          <span>Local Projects in www/</span>
          <span class="pill mono"><?= count($projects) ?></span>
        </div>
        <input
          id="projectSearch"
          type="search"
          class="input mono"
          placeholder="Filter projects..."
          oninput="filterProjects(this.value)"
        >
      </div>

      <?php if (empty($projects)): ?>
        <div class="empty-box">
          <div style="font-weight: 600; color: var(--text-primary);">No project folders inside www/ yet</div>
          <div class="mono" style="margin-top: 6px; font-size: 11px;">
            Create a folder like <span style="color: var(--accent);"><?= es_h($docRootNormalized) ?>/my-app</span> to serve it automatically.
          </div>
        </div>
      <?php else: ?>
        <div class="projects-grid" id="projectsGrid">
          <?php foreach ($projects as $proj): ?>
            <div class="project-card" data-name="<?= es_h(strtolower($proj['name'] . ' ' . $proj['domain'] . ' ' . $proj['framework'])) ?>">
              <div class="project-meta">
                <div class="project-name">
                  <span><?= es_h($proj['name']) ?></span>
                  <span class="tag mono"><?= es_h($proj['framework']) ?></span>
                  <?php if ($proj['doc_suffix'] !== ''): ?>
                    <span class="tag mono" style="color: var(--green);"><?= es_h($proj['doc_suffix']) ?></span>
                  <?php endif; ?>
                </div>
                <div class="project-domain mono">
                  <a href="<?= es_h($proj['primary_url']) ?>" target="_blank" rel="noopener">
                    <?= es_h($proj['primary_url']) ?>
                  </a>
                </div>
              </div>
              <div class="project-actions">
                <a href="<?= es_h($proj['sub_url']) ?>" class="btn mono">Folder</a>
                <a href="<?= es_h($proj['primary_url']) ?>" target="_blank" rel="noopener" class="btn btn-primary mono">Open</a>
              </div>
            </div>
          <?php endforeach; ?>
        </div>
      <?php endif; ?>
    </section>

    <section class="panel">
      <div class="section-head">
        <div class="section-title">
          <span>Loaded PHP Extensions</span>
          <span class="pill mono"><?= $loadedExtCount ?> total</span>
        </div>
        <a href="?q=info" class="mono" style="font-size: 11px; color: var(--accent);">View full phpinfo() &rarr;</a>
      </div>
      <div class="ext-grid">
        <?php foreach ($keyExtensions as $ext): $ok = extension_loaded($ext); ?>
          <span class="ext-pill mono">
            <span class="dot <?= $ok ? 'dot-ok' : '' ?>"></span>
            <span><?= es_h($ext) ?></span>
          </span>
        <?php endforeach; ?>
      </div>
    </section>

    <footer class="footer mono">
      <span>EnvStackUI &middot; Portable Local Web Stack</span>
      <span>AKZ Team</span>
    </footer>
  </main>

  <script>
    (function () {
      var saved = null;
      try { saved = localStorage.getItem('envstackui-home-theme'); } catch (e) {}
      if (saved === 'dark' || saved === 'light') {
        document.documentElement.setAttribute('data-theme', saved);
      }
      updateThemeLabel();
    })();

    function updateThemeLabel() {
      var btn = document.getElementById('themeToggle');
      if (!btn) return;
      var current = document.documentElement.getAttribute('data-theme') || 'dark';
      btn.textContent = current === 'dark' ? 'One Dark' : 'One Light';
    }

    function toggleTheme() {
      var current = document.documentElement.getAttribute('data-theme') || 'dark';
      var next = current === 'dark' ? 'light' : 'dark';
      document.documentElement.setAttribute('data-theme', next);
      try { localStorage.setItem('envstackui-home-theme', next); } catch (e) {}
      updateThemeLabel();
    }

    function filterProjects(query) {
      var q = (query || '').trim().toLowerCase();
      var cards = document.querySelectorAll('#projectsGrid .project-card');
      for (var i = 0; i < cards.length; i++) {
        var hay = cards[i].getAttribute('data-name') || '';
        cards[i].style.display = !q || hay.indexOf(q) !== -1 ? 'flex' : 'none';
      }
    }
  </script>
</body>
</html>
