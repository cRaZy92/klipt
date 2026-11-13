import { describe, expect, it } from "vitest";
import { removalChoices } from "./removal";

const clip = (uploaded: boolean, fileMissing = false) => ({ uploaded, fileMissing });

describe("removalChoices", () => {
  it("one uploaded clip: all three, deleting from this PC first", () => {
    expect(removalChoices({ local: [clip(true)], onlineOnly: 0 })).toEqual({
      library: true,
      local: true,
      everywhere: true,
      initial: "local",
      notUploaded: 0,
    });
  });

  it("a clip that isn't uploaded starts at library only and has nothing to delete everywhere", () => {
    const c = removalChoices({ local: [clip(false)], onlineOnly: 0 });
    expect(c).toMatchObject({ everywhere: false, initial: "library", notUploaded: 1 });
  });

  it("a mix starts at library only when any clip would be lost", () => {
    const c = removalChoices({ local: [clip(true), clip(false)], onlineOnly: 0 });
    expect(c).toMatchObject({ local: true, everywhere: true, initial: "library", notUploaded: 1 });
  });

  it("missing files offer no delete from this PC", () => {
    const c = removalChoices({ local: [clip(false, true)], onlineOnly: 0 });
    expect(c).toMatchObject({ library: true, local: false, everywhere: false, initial: "library" });
  });

  it("only online uploads: deleting from the server is the one choice", () => {
    const c = removalChoices({ local: [], onlineOnly: 2 });
    expect(c).toMatchObject({ library: false, local: false, everywhere: true, initial: "everywhere" });
  });
});
