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
    wrong_password: "That isn't your current password.",
    invalid_display_name: 'Names are up to 32 characters.',
    invalid_reset: 'This password link was used already or has expired. Ask your admin for a new one.',
    password_mismatch: "The two new passwords aren't the same.",
    album_full: 'That album is full (200 clips).',
    album_changed: 'The album changed meanwhile. Reload the page and try again.',
    unknown_clip: 'That clip no longer exists.',
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
      var kind = form.dataset.form;
      if ((kind === 'password' || kind === 'reset') && data.newPassword !== data.confirmPassword) {
        throw new Error('password_mismatch');
      }
      if (kind === 'profile') {
        var me = await api('PATCH', '/api/me', { displayName: data.displayName });
        document.querySelectorAll('.topbar .who').forEach(function (who) {
          var label = who.querySelector('span:not(.avatar)');
          if (label) label.textContent = me.displayName;
          var avatar = who.querySelector('.avatar');
          if (avatar) {
            // same initial and hue as Avatar() in src/web/layout.tsx
            var hash = 0;
            for (var ch of me.displayName.toLowerCase()) hash = (hash * 31 + ch.codePointAt(0)) >>> 0;
            avatar.textContent = ([...me.displayName][0] || '?').toUpperCase();
            avatar.style.setProperty('--h', String(hash % 360));
          }
        });
        form.querySelector('[name=displayName]').value = me.customName || '';
        toast('Name saved');
        btn.disabled = false;
        return;
      }
      if (kind === 'password') {
        await api('POST', '/api/me/password', { currentPassword: data.currentPassword, newPassword: data.newPassword });
        form.reset();
        toast('Password changed. Other devices are signed out.');
        btn.disabled = false;
        return;
      }
      if (kind === 'feedback') {
        await api('POST', '/api/feedback', { kind: data.kind, message: data.message, source: 'web' });
        form.reset();
        toast('Sent. Thanks!');
        btn.disabled = false;
        return;
      }
      if (kind === 'album') {
        var made = await api('POST', '/api/albums', { title: data.title });
        location.assign('/a/' + encodeURIComponent(made.id));
        return;
      }
      if (kind === 'reset') {
        await api('POST', '/api/password-reset', { code: data.code, newPassword: data.newPassword });
      } else if (kind === 'login') {
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
      } else if (action === 'album-pick') {
        await openAlbumPicker();
      } else if (action === 'album-rename') {
        var album = btn.closest('[data-album-id]');
        var title = prompt('Album name', btn.dataset.title);
        if (title === null) return;
        title = title.trim();
        if (!title || title.length > 100) { toast('Album names are 1-100 characters'); return; }
        await api('PATCH', '/api/albums/' + album.dataset.albumId, { title: title });
        album.querySelector('[data-role=album-title]').textContent = title;
        btn.dataset.title = title;
        document.title = title + ' · Klipt';
        toast('Renamed');
      } else if (action === 'album-delete') {
        if (!confirm('Delete this album? Its clips stay where they are.')) return;
        await api('DELETE', '/api/albums/' + btn.closest('[data-album-id]').dataset.albumId);
        location.assign('/albums');
      } else if (action === 'album-move') {
        var item = btn.closest('[data-album-clip]');
        var ids = Array.prototype.map.call(item.parentElement.querySelectorAll('[data-album-clip]'), function (li) { return li.dataset.albumClip; });
        var from = ids.indexOf(item.dataset.albumClip);
        var to = from + Number(btn.dataset.dir);
        if (to < 0 || to >= ids.length) return;
        ids.splice(to, 0, ids.splice(from, 1)[0]);
        await api('PATCH', '/api/albums/' + btn.closest('[data-album-id]').dataset.albumId, { clipIds: ids });
        location.reload();
      } else if (action === 'album-remove') {
        var gone = btn.closest('[data-album-clip]');
        await api('DELETE', '/api/albums/' + btn.closest('[data-album-id]').dataset.albumId + '/clips/' + gone.dataset.albumClip);
        location.reload();
      } else if (action === 'retry-transcode') {
        btn.disabled = true;
        await api('POST', '/api/admin/transcodes/' + encodeURIComponent(clipRow.dataset.clipId) + '/retry');
        await refreshTranscodes();
        toast('Back in the queue');
      } else if (action === 'reset-password') {
        var ru = btn.closest('[data-user-id]');
        var reset = await api('POST', '/api/admin/users/' + ru.dataset.userId + '/password-reset');
        await copy(reset.url);
        toast('Password link copied. Send it to them; it works once, for a day.');
      } else if (action === 'resolve-feedback') {
        var fr = btn.closest('[data-feedback-id]');
        var resolve = btn.dataset.resolved !== 'true';
        await api('PATCH', '/api/admin/feedback/' + fr.dataset.feedbackId, { resolved: resolve });
        location.reload();
      } else if (action === 'delete-feedback') {
        var fd = btn.closest('[data-feedback-id]');
        if (!confirm('Delete this report?')) return;
        await api('DELETE', '/api/admin/feedback/' + fd.dataset.feedbackId);
        fd.remove();
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

  // A clip's page: "Add to album" lists your albums with a check box each, and makes new ones.
  function albumPicker() { return document.querySelector('dialog.album-picker'); }

  async function openAlbumPicker() {
    var dialog = albumPicker();
    if (!dialog) return;
    if (!dialog.open) dialog.showModal();
    await renderAlbumChoices(dialog);
  }

  async function renderAlbumChoices(dialog) {
    var list = dialog.querySelector('[data-role=album-choices]');
    var albums = await api('GET', '/api/albums?clip=' + encodeURIComponent(dialog.dataset.clipId));
    list.textContent = '';
    if (albums.length === 0) {
      var none = document.createElement('li');
      none.className = 'muted';
      none.textContent = 'No albums yet. Name one below.';
      list.appendChild(none);
      return;
    }
    albums.forEach(function (a) {
      var li = document.createElement('li');
      var label = document.createElement('label');
      var box = document.createElement('input');
      box.type = 'checkbox';
      box.checked = !!a.hasClip;
      box.dataset.albumId = a.id;
      var name = document.createElement('span');
      name.className = 'album-choice-name';
      name.textContent = a.title;
      var count = document.createElement('span');
      count.className = 'muted';
      count.textContent = a.clipCount === 1 ? '1 clip' : a.clipCount + ' clips';
      var open = document.createElement('a');
      open.href = '/a/' + encodeURIComponent(a.id);
      open.className = 'album-choice-open';
      open.textContent = 'Open';
      label.append(box, name, count);
      li.append(label, open);
      list.appendChild(li);
    });
  }

  document.addEventListener('change', async function (ev) {
    var box = ev.target;
    var dialog = box.closest && box.closest('dialog.album-picker');
    if (!dialog || !box.dataset.albumId) return;
    box.disabled = true;
    try {
      await api(box.checked ? 'PUT' : 'DELETE', '/api/albums/' + box.dataset.albumId + '/clips/' + dialog.dataset.clipId);
      var count = box.parentElement.querySelector('.muted');
      var n = parseInt(count.textContent, 10) + (box.checked ? 1 : -1);
      count.textContent = n === 1 ? '1 clip' : n + ' clips';
      toast(box.checked ? 'Added to ' + box.nextSibling.textContent : 'Removed from ' + box.nextSibling.textContent);
    } catch (e) {
      box.checked = !box.checked;
      toast(friendly(e));
    } finally {
      box.disabled = false;
    }
  });

  document.addEventListener('submit', async function (ev) {
    var form = ev.target.closest && ev.target.closest('form[data-role=album-new]');
    if (!form) return;
    ev.preventDefault();
    var dialog = form.closest('dialog.album-picker');
    var input = form.querySelector('[name=title]');
    var title = input.value.trim();
    if (!title) return;
    var btn = form.querySelector('button');
    btn.disabled = true;
    try {
      await api('POST', '/api/albums', { title: title, clipIds: [dialog.dataset.clipId] });
      form.reset();
      await renderAlbumChoices(dialog);
      toast('Added to ' + title);
    } catch (e) {
      toast(friendly(e));
    } finally {
      btn.disabled = false;
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
  document.querySelectorAll('.share video, .album-page video').forEach(function (video) {
    var volume = parseFloat(stored(VOLUME_KEY));
    video.volume = volume >= 0 && volume <= 1 ? volume : 0.5;
    video.muted = stored(MUTED_KEY) === 'true';
    video.addEventListener('volumechange', function () {
      store(VOLUME_KEY, String(video.volume));
      store(MUTED_KEY, String(video.muted));
    });
  });

  // Album page: a clip that ends moves on to the next one, which starts playing by itself; the
  // clip playing is scrolled into view in the list.
  var AUTOPLAY_KEY = 'klipt.albumAutoplay';
  document.querySelectorAll('.album-page video').forEach(function (video) {
    var session = null;
    try { session = sessionStorage; } catch (e) { /* storage blocked */ }
    if (session && session.getItem(AUTOPLAY_KEY) === '1') {
      session.removeItem(AUTOPLAY_KEY);
      var playing = video.play();
      if (playing && playing.catch) playing.catch(function () {});
    }
    video.addEventListener('ended', function () {
      if (!video.dataset.nextUrl) return;
      try { if (session) session.setItem(AUTOPLAY_KEY, '1'); } catch (e) { /* just no autoplay */ }
      location.assign(video.dataset.nextUrl);
    });
  });
  var currentItem = document.querySelector('.album-list li.current');
  if (currentItem && currentItem.parentElement.scrollHeight > currentItem.parentElement.clientHeight) {
    currentItem.scrollIntoView({ block: 'nearest' });
  }

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
