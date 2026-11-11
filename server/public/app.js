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
    invalid_credentials: 'Wrong username or password.',
    account_disabled: 'This account is disabled.',
    username_taken: 'That username is taken.',
    invalid_username: 'Usernames are 3-24 letters, numbers or underscores.',
    weak_password: 'Passwords need at least 8 characters.',
    invalid_invite: 'This invite link is no longer valid.',
    rate_limited: 'Too many attempts. Try again in a minute.',
  };
  function friendly(err) {
    if (err.status === 429) return MESSAGES.rate_limited;
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
        await api('POST', '/api/register', { inviteCode: data.inviteCode, username: data.username, password: data.password });
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
      } else if (action === 'visibility') {
        var vis = btn.dataset.visibility === 'feed' ? 'link' : 'feed';
        await api('PATCH', '/api/clips/' + clipRow.dataset.clipId, { visibility: vis });
        btn.dataset.visibility = vis;
        btn.textContent = vis === 'feed' ? 'In feed' : 'Link only';
        toast(vis === 'feed' ? 'Now visible in the feed' : 'Now link-only');
      } else if (action === 'delete') {
        if (!confirm('Delete this clip? This cannot be undone.')) return;
        await api('DELETE', '/api/clips/' + clipRow.dataset.clipId);
        if (btn.dataset.redirect) { location.assign(safeNext(btn.dataset.redirect)); return; }
        clipRow.remove();
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
      } else if (action === 'toggle-user') {
        var u = btn.closest('[data-user-id]');
        var disable = btn.dataset.disabled !== 'true';
        if (disable && !confirm('Disable this user and sign them out everywhere?')) return;
        await api('PATCH', '/api/admin/users/' + u.dataset.userId, { disabled: disable });
        location.reload();
      }
    } catch (e) {
      toast('Failed: ' + (e.message || 'error'));
    }
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
