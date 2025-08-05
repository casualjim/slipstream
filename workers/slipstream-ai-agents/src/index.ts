import * as restate from "@restatedev/restate-sdk-cloudflare-workers/fetch";
import { greeter } from "./features/greeter";


export default restate.endpoint().bind(greeter).handler();
