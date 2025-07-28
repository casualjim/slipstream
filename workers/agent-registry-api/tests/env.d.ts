import type { D1Migration } from "cloudflare:test";
import type { AppEnv } from "../src/types";

export type Env = AppEnv & {
  MIGRATIONS: D1Migration[];
};

declare module "cloudflare:test" {
  interface ProvidedEnv extends Env {
    MIGRATIONS: D1Migration[];
  }
}
