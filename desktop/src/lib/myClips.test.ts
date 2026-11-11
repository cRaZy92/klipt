import { describe, expect, it } from "vitest";
import type { ClipView, MyUpload } from "./api";
import { gameCounts, isUploaded, matchesSearch, mergeClips, visibleClips } from "./myClips";

function local(id: string, at: number, extra: Partial<ClipView> = {}): ClipView {
  return {
    id, path: `/clips/${id}.mp4`, createdAt: at, durationMs: 1000, width: 1920, height: 1080, hasMic: false,
    gameName: "Deadlock", steamAppId: 1, exe: null, title: id, trimStartMs: null, trimEndMs: null,
    gameVolume: 1, micVolume: 1, serverClipId: null, shareUrl: null, visibility: null, uploadId: null,
    uploadOffset: null, favorite: false, thumbnailMs: null, hasVoice: false, voiceVolume: 1, fileMissing: false, thumbPath: null, ...extra,
  };
}

function upload(id: string, at: number, extra: Partial<MyUpload> = {}): MyUpload {
  return {
    id, title: id, gameName: "Deadlock", steamAppId: 1, durationMs: 1000, width: 1920, height: 1080,
    createdAt: new Date(at).toISOString(), owner: { username: "adam" }, shareUrl: `https://k/c/${id}`,
    videoUrl: `https://k/media/${id}.mp4`, thumbnailUrl: `https://k/media/${id}.jpg`, visibility: "feed", views: 0, ...extra,
  };
}

describe("mergeClips", () => {
  it("lists an uploaded local clip once, as the local clip", () => {
    const items = mergeClips([local("a", 1, { serverClipId: "S1", shareUrl: "https://k/c/S1" })], [upload("S1", 1)]);
    expect(items.map((i) => i.key)).toEqual(["l:a"]);
    expect(isUploaded(items[0])).toBe(true);
  });

  it("shows uploads with no local copy, or whose local file is gone, as online clips", () => {
    const items = mergeClips(
      [local("gone", 2, { serverClipId: "S2", shareUrl: "https://k/c/S2", fileMissing: true }), local("mine", 3)],
      [upload("S1", 1), upload("S2", 2)],
    );
    expect(items.map((i) => i.key).sort()).toEqual(["l:mine", "s:S1", "s:S2"]);
  });

  it("keeps a missing local clip that isn't online, so it can be removed", () => {
    const items = mergeClips([local("lost", 1, { fileMissing: true })], []);
    expect(items.map((i) => i.key)).toEqual(["l:lost"]);
  });
});

describe("visibleClips", () => {
  const items = mergeClips(
    [local("new", 30), local("up", 20, { serverClipId: "S9", shareUrl: "https://k/c/S9", gameName: "CS2" })],
    [upload("S9", 20), upload("old", 10)],
  );

  it("filters by place", () => {
    const keys = (place: "all" | "local" | "uploaded") =>
      visibleClips(items, place, null, "newestFirst").map((i) => i.key);
    expect(keys("all")).toEqual(["l:new", "l:up", "s:old"]);
    expect(keys("local")).toEqual(["l:new", "l:up"]);
    expect(keys("uploaded")).toEqual(["l:up", "s:old"]);
  });

  it("filters by game and sorts both ways", () => {
    expect(visibleClips(items, "all", "CS2", "newestFirst").map((i) => i.key)).toEqual(["l:up"]);
    expect(visibleClips(items, "all", null, "oldestFirst").map((i) => i.key)).toEqual(["s:old", "l:up", "l:new"]);
  });

  it("counts games for the chips, most clips first", () => {
    expect(gameCounts(items)).toEqual([
      { name: "Deadlock", steamAppId: 1, count: 2 },
      { name: "CS2", steamAppId: 1, count: 1 },
    ]);
  });
});

describe("favorites and search", () => {
  it("Favorites shows only starred clips on this PC", () => {
    const items = mergeClips([local("star", 2, { favorite: true }), local("plain", 1)], [upload("S1", 1)]);
    expect(visibleClips(items, "favorites", null, "newestFirst").map((i) => i.key)).toEqual(["l:star"]);
  });

  it("matches every word in the title or game, ignoring case and accents", () => {
    const items = mergeClips(
      [local("a", 3, { title: "Clutch ACE", gameName: "Counter-Strike 2" }), local("b", 2, { title: "Útek z väzenia", gameName: null })],
      [upload("S1", 1, { title: "Team wipe", gameName: "Active Matter" })],
    );
    const keys = (q: string) => visibleClips(items, "all", null, "newestFirst", q).map((i) => i.key);
    expect(keys("ace")).toEqual(["l:a"]);
    expect(keys("counter clutch")).toEqual(["l:a"]);
    expect(keys("matter")).toEqual(["s:S1"]);
    expect(keys("utek vazenia")).toEqual(["l:b"]);
    expect(keys("ace wipe")).toEqual([]);
    expect(keys("  ")).toHaveLength(3);
  });

  it("matchesSearch works on plain text", () => {
    expect(matchesSearch(["Žltý Kôň", null], "zlty kon")).toBe(true);
    expect(matchesSearch(["x"], "y")).toBe(false);
  });
});
