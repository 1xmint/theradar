// SPDX-License-Identifier: Apache-2.0
//! Whether trading is switched on, server-side -- the second of the two dark
//! switches `TradePanel` checks (the first is `legal.ts`'s `TERMS_APPROVED`).
//!
//! Modelled as a plain boolean, not a `Load<T>` union like `useApi`: nothing
//! downstream needs to tell "still loading" apart from "off" or "could not
//! read health" apart from "off" -- all three mean the same thing to a
//! visitor, "no trade panel here", and inventing three renderable states for
//! a fact that only ever gates visibility would be complexity with no
//! reader. **The direction that matters is the one this collapses toward**:
//! every non-`true` outcome reads as `false`, never as `true`, so a slow or
//! failed health check can only ever hide the button, not show it before the
//! server actually said so.

import { useEffect, useState } from "react";
import { health } from "./api";

/** Whether `/health` says trading is on. Starts `false` and only ever
 *  becomes `true` on an explicit `trading: true` from the server. */
export function useTrading(): boolean {
  const [trading, setTrading] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    health(controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) setTrading(value.trading === true);
      })
      .catch(() => {
        // A failed or unreachable health check is not evidence trading is
        // on. It stays `false`, the state it started in.
      });
    return () => controller.abort();
  }, []);

  return trading;
}
