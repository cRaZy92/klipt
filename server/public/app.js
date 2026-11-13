// Klipt web UI: tiny vanilla helpers. Everything goes through the JSON API.
(function () {
  'use strict';

  function toast(msg) {
    var el = document.getElementById('toast');
    if (!el) return;
    el.textContent = msg;
    el.hidden = false;
    clearTimeout(toast.t);
    toast.t = setTimeout(function () { el.hidden = true; }, 2500);
  }

  async function api(method, url, body) {
    var res = await fetch(url, {
      method: method,
      headers: body ? { 'content-type': 'application/json' } : {},
      body: body ? JSON.stringify(body) : undefined,
      credentials: 'same-origin',
    });
    if (!res.ok) {
      var code = '';
      try { code = (await res.json()).error || ''; } catch (e) { /* no body */ }
      var err = new Error(code || 'request_failed');
      err.status = res.status;
      throw err;
    }
    return res.status === 204 ? null : res.json();
  }

  var MESSAGES = {
    unknown_user: "One of those names isn't a Klipt user.",
    invalid_credentials: 'Wrong username or password.',
    account_disabled: 'This account is disabled.',
    username_taken: 'That username is taken.',
    invalid_username: 'Usernames are 3-24 letters, numbers or underscores.',
    weak_password: 'Passwords need at least 8 characters.',
    invalid_invite: 'This invite link is no longer valid.',
    invalid_share: "This clip can't be used to sign up anymore. Ask a friend for an invite link.",
    rate_limited: 'Too many attempts. Try again in a minute.',
    unauthorized: 'You are logged out. Log in again and retry.',
    forbidden: "You don't have permission to do that.",
    not_found: 'That no longer exists. Reload the page.',
    cannot_disable_self: "You can't disable your own account.",
    invalid_request: "That didn't work. Reload the page and try again.",
  };
  function friendly(err) {
    if (err.status === 429) return MESSAGES.rate_limited;
    // fetch itself failed: no response at all.
    if (err.status === undefined && err instanceof TypeError) return "Can't reach Klipt. Check your connection.";
    return MESSAGES[err.message] || 'Something went wrong. Please try again.';
  }

  function safeNext(n) {
    return typeof n === 'string' && /^\/(?![\/\\])[\u0020-\u005b\u005d-\u007e]*$/.test(n) ? n : '/';
  }

  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      var ta = document.createElement('textarea');
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy'); } finally { ta.remove(); }
    }
    toast('Link copied');
  }

  // Login + invite signup forms
  document.addEventListener('submit', async function (ev) {
    var form = ev.target.closest && ev.target.closest('form[data-form]');
    if (!form) return;
    ev.preventDefault();
    var errEl = form.querySelector('.error');
    var btn = form.querySelector('button[type=submit]');
    var data = Object.fromEntries(new FormData(form).entries());
    errEl.hidden = true;
    btn.disabled = true;
    try {
      if (form.dataset.form === 'login') {
        await api('POST', '/api/auth/login', { username: data.username, password: data.password, client: 'web' });
      } else {
        // From an admin's invite link, or from a clip's share page (its owner invites).
        var reg = { username: data.username, password: data.password };
        if (data.clipId) reg.clipId = data.clipId; else reg.inviteCode = data.inviteCode;
        await api('POST', '/api/register', reg);
      }
      location.assign(safeNext(form.dataset.next));
    } catch (e) {
      errEl.textContent = friendly(e);
      errEl.hidden = false;
      btn.disabled = false;
    }
  });

  document.addEventListener('click', async function (ev) {
    var btn = ev.target.closest && ev.target.closest('[data-action]');
    if (!btn) return;
    var action = btn.dataset.action;
    var clipRow = btn.closest('[data-clip-id]');
    try {
      if (action === 'copy') {
        await copy(btn.dataset.url);
      } else if (action === 'logout') {
        await api('POST', '/api/auth/logout');
        location.assign('/login');
      } else if (action === 'rename') {
        var next = prompt('New title', btn.dataset.title);
        if (next === null) return;
        next = next.trim();
        if (!next || next.length > 100) { toast('Titles are 1-100 characters'); return; }
        await api('PATCH', '/api/clips/' + clipRow.dataset.clipId, { title: next });
        clipRow.querySelector('[data-role=title]').textContent = next;
        btn.dataset.title = next;
        toast('Renamed');
      } else if (action === 'tag') {
        var names = prompt('Who is in this clip? Usernames, separated by commas.', btn.dataset.tagged);
        if (names === null) return;
        var list = names.split(',').map(function (n) { return n.trim(); }).filter(Boolean);
        await api('PATCH', '/api/clips/' + clipRow.dataset.clipId, { tagged: list });
        location.reload();
      } else if (action === 'visibility') {
        var vis = btn.dataset.visibility === 'feed' ? 'link' : 'feed';
        await api('PATCH', '/api/clips/' + clipRow.dataset.clipId, { visibility: vis });
        btn.dataset.visibility = vis;
        btn.textContent = vis === 'feed' ? 'Hide from feed' : 'Show in feed';
        var badge = clipRow.querySelector('[data-role=visibility]');
        if (badge) badge.textContent = vis === 'feed' ? 'In feed' : 'Link only';
        toast(vis === 'feed' ? 'Now visible in the feed' : 'Now link-only');
      } else if (action === 'delete') {
        if (!confirm('Delete this clip? This cannot be undone.')) return;
        await api('DELETE', '/api/clips/' + clipRow.dataset.clipId);
        if (btn.dataset.redirect) { location.assign(safeNext(btn.dataset.redirect)); return; }
        var list = clipRow.parentElement;
        clipRow.remove();
        // The last one on the page: reload for the empty state (or the next page of clips).
        if (list && !list.querySelector('[data-clip-id]')) { location.reload(); return; }
        toast('Deleted');
      } else if (action === 'revoke-session') {
        var s = btn.closest('[data-session-id]');
        if (!confirm('Sign out this session?')) return;
        await api('DELETE', '/api/auth/sessions/' + s.dataset.sessionId);
        s.remove();
      } else if (action === 'create-invite') {
        var inv = await api('POST', '/api/admin/invites');
        await copy(inv.url);
        location.reload();
      } else if (action === 'revoke-invite') {
        var row = btn.closest('[data-code]');
        if (!confirm('Revoke this invite?')) return;
        await api('DELETE', '/api/admin/invites/' + encodeURIComponent(row.dataset.code));
        row.remove();
      } else if (action === 'favorite') {
        var on = btn.getAttribute('aria-pressed') !== 'true';
        await api(on ? 'PUT' : 'DELETE', '/api/clips/' + clipRow.dataset.clipId + '/favorite');
        btn.setAttribute('aria-pressed', String(on));
        btn.classList.toggle('on', on);
        btn.textContent = on ? '★ Favorite' : '☆ Favorite';
        toast(on ? 'Added to favorites' : 'Removed from favorites');
      } else if (action === 'retry-transcode') {
        btn.disabled = true;
        await api('POST', '/api/admin/transcodes/' + encodeURIComponent(clipRow.dataset.clipId) + '/retry');
        await refreshTranscodes();
        toast('Back in the queue');
      } else if (action === 'toggle-user') {
        var u = btn.closest('[data-user-id]');
        var disable = btn.dataset.disabled !== 'true';
        if (disable && !confirm('Disable this user and sign them out everywhere?')) return;
        await api('PATCH', '/api/admin/users/' + u.dataset.userId, { disabled: disable });
        location.reload();
      }
    } catch (e) {
      toast(friendly(e));
    }
  });

  // Admin page: the compression queue refreshes itself, every few seconds while something is
  // compressing and every half minute otherwise (to notice new uploads). Not while the tab is hidden.
  async function refreshTranscodes() {
    var section = document.getElementById('transcodes');
    if (!section) return;
    var res = await fetch('/admin/transcodes', { credentials: 'same-origin', cache: 'no-store' });
    // Logged out meanwhile: a redirect to the login page, which must not land inside the admin page.
    if (!res.ok || res.redirected) return;
    var holder = document.createElement('div');
    holder.innerHTML = await res.text();
    var fresh = holder.querySelector('#transcodes');
    if (!fresh) return;
    // Keep any error details the admin opened.
    section.querySelectorAll('details[open]').forEach(function (d) {
      var row = d.closest('[data-clip-id]');
      var again = row && fresh.querySelector('[data-clip-id="' + row.dataset.clipId + '"] details');
      if (again) again.open = true;
    });
    section.replaceWith(fresh);
  }
  if (document.getElementById('transcodes')) {
    (function poll() {
      var section = document.getElementById('transcodes');
      var wait = section && section.dataset.busy === 'true' ? 3000 : 30000;
      setTimeout(async function () {
        if (!document.hidden) {
          try { await refreshTranscodes(); } catch (e) { /* offline for a moment: try again next round */ }
        }
        poll();
      }, wait);
    })();
  }

  // Share page: the first play counts as a view (the server counts each friend once).
  document.querySelectorAll('video[data-view-url]').forEach(function (video) {
    video.addEventListener('play', function () {
      fetch(video.dataset.viewUrl, { method: 'POST', credentials: 'same-origin' }).catch(function () {});
    }, { once: true });
  });

  // Share page: the clip plays when the visitor starts it, at half volume unless they've changed
  // it before; their volume and mute are remembered in this browser.
  var VOLUME_KEY = 'klipt.volume';
  var MUTED_KEY = 'klipt.muted';
  function stored(key) {
    try { return localStorage.getItem(key); } catch (e) { return null; }
  }
  function store(key, value) {
    try { localStorage.setItem(key, value); } catch (e) { /* private mode: just not remembered */ }
  }
  document.querySelectorAll('.share video').forEach(function (video) {
    var volume = parseFloat(stored(VOLUME_KEY));
    video.volume = volume >= 0 && volume <= 1 ? volume : 0.5;
    video.muted = stored(MUTED_KEY) === 'true';
    video.addEventListener('volumechange', function () {
      store(VOLUME_KEY, String(video.volume));
      store(MUTED_KEY, String(video.muted));
    });
  });

  // A screenshot counts as viewed when its page opens.
  document.querySelectorAll('img[data-view-url]').forEach(function (img) {
    fetch(img.dataset.viewUrl, { method: 'POST', credentials: 'same-origin' }).catch(function () {});
  });

  // Game chips: drop the art when Steam has none, so the chip shows just its name.
  // Errors don't bubble, hence the capture listener; the sweep catches any that failed before this ran.
  function dropBrokenArt(img) {
    if (img.matches && img.matches('img.chip-art')) img.remove();
  }
  document.addEventListener('error', function (ev) { dropBrokenArt(ev.target); }, true);
  document.querySelectorAll('img.chip-art').forEach(function (img) {
    if (img.complete && img.naturalWidth === 0) dropBrokenArt(img);
  });
})();
