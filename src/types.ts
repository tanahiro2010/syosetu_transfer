export type Platform = "kakuyomu" | "narou";

export type WorkInfo = {
  platform: Platform;
  title: string;
  author_name?: string | null;
  episode_count: number;
  first_url?: string | null;
  source_ref: string;
};

export type Episode = {
  number: number;
  title: string;
  content: string;
  url?: string | null;
};

export type TransferDraft = {
  work: WorkInfo;
  episodes: Episode[];
};

export type PostMode = "confirm" | "auto";

/** convert: 投稿先の記法に変換する / strip: ルビと傍点を外して本文だけにする */
export type RubyMode = "convert" | "strip";

export type Settings = {
  postMode: PostMode;
  rubyMode: RubyMode;
  postIntervalSec: number;
  fetchIntervalSec: number;
  defaultSource: Platform;
  defaultTarget: Platform;
  managementUrls: Record<Platform, string>;
};

export type FetchProgress = {
  current: number;
  total: number | null;
  title: string;
};

export type EpisodeStatus =
  | "opening"
  | "login-required"
  | "filled"
  | "waiting-user"
  | "submitting"
  | "posted"
  | "submitted"
  | "skipped"
  | "error";

export type PostProgress = {
  index: number | null;
  status: EpisodeStatus | "done" | "stopped";
  message: string;
};

export const platforms: Record<Platform, string> = {
  kakuyomu: "カクヨム",
  narou: "小説家になろう",
};

export function platformEntries(): [Platform, string][] {
  return Object.entries(platforms) as [Platform, string][];
}
