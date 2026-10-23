/** Compact in-memory D1 emulating the exact SQL shapes the Worker issues (shared by e2e suites). */

export function miniD1() {
  const t = {
    profiles: new Map<string, any>(),
    devices: new Map<string, any>(),
    entitlements: new Map<string, any>(),
    profile_events: [] as any[],
    identity_migration: new Map<string, any>(),
    oauth_tokens: new Map<string, any>(),
    usage_log: new Map<string, any>(),
  };

  const exec = (sql: string, args: any[]) => {
    const s = sql.replace(/\s+/g, " ").trim();

    // SELECT ... FROM X WHERE ... (first/all)
    if (s.startsWith("SELECT")) {
      const table = (s.match(/FROM (\w+)/) || [])[1];
      const isCount = /COUNT\(\*\) AS n/.test(s);
      const isCoalesce = /COALESCE\(SUM\(ai_neurons\), 0\)/.test(s);
      const whereProfile = /WHERE profile_id = \?/.test(s);
      const whereHint = /WHERE provision_hint = \?/.test(s);
      const whereDeviceAndProfile = /WHERE device_id = \? AND profile_id = \?/.test(s);
      const whereDevice = /WHERE device_id = \?/.test(s);
      const whereScope = /WHERE profile_id = \? AND scope = \?/.test(s);
      const whereUserDay = /WHERE user_id = \? AND day_utc = \?/.test(s);
      const whereDay = /WHERE day_utc = \?/.test(s);
      const whereUser = /WHERE user_id = \?/.test(s);
      const whereLegacy = /WHERE legacy_user_id = \?/.test(s);
      const rowsFor = (tbl: string, pred: (r: any) => boolean) =>
        [...(t as any)[tbl].values()].filter(pred);

      const count = (rows: any[]) => ({ n: rows.length });

      if (table === "profiles") {
        if (isCount) return { first: async () => count(rowsFor("profiles", r => r.status === "pending")) };
        if (whereHint) return { first: async () => rowsFor("profiles", r => r.provision_hint === args[0])
          .sort((a, b) => b.created_at - a.created_at)[0] || null };
        if (whereProfile) return { first: async () => t.profiles.get(args[0]) || null };
      }
      if (table === "devices") {
        if (whereDeviceAndProfile) return { first: async () => {
          const d = t.devices.get(args[0]);
          return d && d.profile_id === args[1] ? d : null;
        } };
        if (whereDevice) return { first: async () => t.devices.get(args[0]) || null };
        if (whereProfile) return { all: async () => ({ results: rowsFor("devices", r => r.profile_id === args[0])
          .sort((a, b) => a.created_at - b.created_at) }) };
      }
      if (table === "entitlements") {
        if (whereScope) return { first: async () => t.entitlements.get(`${args[0]}:${args[1]}`) || null };
        if (whereProfile) return { all: async () => ({ results: rowsFor("entitlements", r => r.profile_id === args[0]) }) };
      }
      if (table === "identity_migration") {
        if (whereLegacy) return { first: async () => t.identity_migration.get(args[0]) || null };
      }
      if (table === "oauth_tokens") {
        // getValid{Github,Google,Swiggy}Token: provider is a literal in the SQL.
        const prov = (s.match(/provider = '(\w+)'/) || [])[1];
        if (prov && /SELECT access_token, refresh_token, expires_at/.test(s)) {
          return { first: async () => t.oauth_tokens.get(`${args[0]}:${prov}`) || null };
        }
        if (isCount && whereUser) return { first: async () => count(rowsFor("oauth_tokens", r => r.user_id === args[0])) };
        if (whereUserDay) return { first: async () => t.usage_log.get(`${args[0]}:${args[1]}`) || null };
      }
      if (table === "usage_log") {
        if (isCoalesce && whereDay) return { first: async () => {
          let total = 0;
          for (const r of t.usage_log.values()) if (r.day_utc === args[0]) total += r.ai_neurons || 0;
          return { total };
        } };
        if (whereUserDay) return { first: async () => t.usage_log.get(`${args[0]}:${args[1]}`) || null };
      }
      if (table === "profile_events") {
        if (isCount) {
          return { first: async () => {
            const [pattern, since] = args;
            const m = pattern.match(/"ip":"(.+?)"/);
            const ip = m ? m[1] : null;
            let n = 0;
            for (const e of t.profile_events) {
              if (e.event !== "registered" || e.at <= since) continue;
              try { if (JSON.parse(e.detail || "{}").ip === ip) n++; } catch { /* skip */ }
            }
            return { n };
          } };
        }
      }
      return { first: async () => null, all: async () => ({ results: [] }) };
    }

    if (s.startsWith("INSERT")) {
      const table = (s.match(/INTO (\w+)/) || [])[1];
      if (table === "profiles") {
        t.profiles.set(args[0], {
          profile_id: args[0], human_label: args[1], status: args[2], quota_tier: args[3],
          created_at: args[4], approved_at: args[5], approved_by: args[6], expires_at: args[7],
          last_seen_at: args[8], provision_hint: args[9],
        });
      } else if (table === "devices") {
        t.devices.set(args[0], {
          device_id: args[0], profile_id: args[1], device_token_hash: args[2],
          device_name: args[3], os: args[4], app_version: args[5], status: args[6],
          created_at: args[7], last_seen_at: args[8], revoked_at: args[9],
        });
      } else if (table === "profile_events") {
        t.profile_events.push({ profile_id: args[0], device_id: args[1], event: args[2], detail: args[3], at: args[4] });
      } else if (table === "entitlements") {
        t.entitlements.set(`${args[0]}:${args[1]}`, {
          profile_id: args[0], scope: args[1], allowed: args[2], quota_tier: args[3],
          granted_by: args[4], granted_at: args[5], expires_at: args[6],
        });
      } else if (table === "identity_migration") {
        t.identity_migration.set(args[0], { legacy_user_id: args[0], profile_id: args[1] });
      } else if (table === "usage_log") {
        // Emulates INSERT ... ON CONFLICT DO UPDATE SET col = col + excluded.col
        const prev = t.usage_log.get(`${args[0]}:${args[1]}`) || {
          requests: 0, ai_neurons: 0, d1_reads: 0, d1_writes: 0, search_calls: 0, deep_calls: 0,
        };
        t.usage_log.set(`${args[0]}:${args[1]}`, {
          user_id: args[0], day_utc: args[1], requests: prev.requests + args[2],
          ai_neurons: prev.ai_neurons + args[3], d1_reads: prev.d1_reads + args[4],
          d1_writes: prev.d1_writes + args[5], search_calls: prev.search_calls + args[6],
          deep_calls: prev.deep_calls + args[7],
        });
      }
      return { run: async () => {} };
    }

    if (s.startsWith("UPDATE")) {
      const table = (s.match(/UPDATE (\w+)/) || [])[1];
      if (table === "profiles") {
        const p = t.profiles.get(args[args.length - 1]);
        if (p) {
          if (/SET last_seen_at = \?/.test(s) && !/status/.test(s)) p.last_seen_at = args[0];
          else {
            // dynamic patch: parse "SET col = ?, col = ?"
            const setPart = s.slice(s.indexOf("SET") + 3, s.indexOf("WHERE"));
            const cols = (setPart.match(/(\w+) = \?/g) || []).map(x => x.replace(/ = \?/, ""));
            cols.forEach((c, i) => { p[c] = args[i]; });
          }
        }
      } else if (table === "devices") {
        const d = t.devices.get(args[args.length - 1]);
        if (d) {
          if (/device_token_hash/.test(s)) { d.device_token_hash = args[0]; d.last_seen_at = args[1]; }
          else if (/status = 'revoked'/.test(s)) { d.status = "revoked"; d.revoked_at = args[0]; d.last_seen_at = args[1]; }
        }
      }
      return { run: async () => {} };
    }

    if (s.startsWith("DELETE")) {
      const table = (s.match(/FROM (\w+)/) || [])[1];
      if (table === "oauth_tokens") {
        for (const [k, r] of [...t.oauth_tokens]) if (r.user_id === args[0]) t.oauth_tokens.delete(k);
      }
      return { run: async () => {} };
    }

    throw new Error(`miniD1: unsupported SQL: ${s}`);
  };

  return {
    prepare: (sql: string) => {
      const runWith = (args: any[]) => exec(sql, args);
      return {
        // Zero-bind queries (e.g. countPending uses prepare(...).first()).
        first: async () => {
          const out = runWith([]);
          return ("first" in out) ? out.first() : null;
        },
        all: async () => {
          const out = runWith([]);
          return ("all" in out) ? out.all() : { results: [] };
        },
        run: async () => {
          const out = runWith([]);
          if ("run" in out) await out.run();
        },
        bind: (...args: any[]) => {
          const out = runWith(args);
          return {
            first: async () => ("first" in out) ? out.first() : null,
            all: async () => ("all" in out) ? out.all() : { results: [] },
            run: async () => { if ("run" in out) await out.run(); },
          };
        },
      };
    },
    _t: t,
  };
}
