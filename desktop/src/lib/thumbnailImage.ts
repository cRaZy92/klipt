import { open } from "@tauri-apps/plugin-dialog";

/** Asks for a picture to use as a clip's thumbnail; null when cancelled. */
export async function pickThumbnailImage(): Promise<string | null> {
  const picked = await open({
    title: "Choose a picture for the thumbnail",
    filters: [{ name: "Pictures", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  return typeof picked === "string" ? picked : null;
}
