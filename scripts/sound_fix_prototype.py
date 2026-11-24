"""Feasibility prototype: fix a mis-heard command by SOUND (phoneme distance) + SENTENCE SHAPE (command grammar) + PRIORS.
Not production: OOV words use hand pronunciations (production would use the G2P already in the repo, misaki-rs)."""
import re, cmudict, json, sys

CMU = cmudict.dict()
HAND = {  # words CMUdict lacks (brands, NEXUS vocabulary)
    "nexus": "N EH K S AH S", "spotify": "S P AA T AH F AY", "whatsapp": "W AA T S AE P", "antigravity": "AE N T IY G R AE V AH T IY",
    "github": "G IH T HH AH B", "vscode": "V IY EH S K OW D", "notepad": "N OW T P AE D", "brave": "B R EY V", "chrome": "K R OW M",
    "discord": "D IH S K AO R D", "youtube": "Y UW T UW B", "opencode": "OW P AH N K OW D", "copilot": "K OW P AY L AH T",
    "cursor": "K ER S ER", "codex": "K OW D EH K S", "gemini": "JH EH M AH N AY", "ghost": "G OW S T", "vs": "V IY EH S",
    "laptop": "L AE P T AA P", "shutdown": "SH AH T D AW N", "anti": "AE N T IY",
}
LETTER = {"a": "AE", "b": "B", "c": "K", "d": "D", "e": "EH", "f": "F", "g": "G", "h": "HH", "i": "IH", "j": "JH", "k": "K", "l": "L", "m": "M",
          "n": "N", "o": "AA", "p": "P", "q": "K", "r": "R", "s": "S", "t": "T", "u": "AH", "v": "V", "w": "W", "x": "K S", "y": "Y", "z": "Z"}


def phones(word):
    w = re.sub(r"[^a-z]", "", word.lower())
    if not w:
        return []
    if w in HAND:
        return HAND[w].split()
    if w in CMU:
        return [re.sub(r"\d", "", p) for p in CMU[w][0]]
    return [p for ch in w for p in LETTER[ch].split()]  # crude fallback


VOWELS = set("AA AE AH AO AW AY EH ER EY IH IY OW OY UH UW".split())
# groups that sound alike: substituting inside a group is cheap
GROUPS = [set("P B".split()), set("T D".split()), set("K G".split()), set("F V".split()), set("S Z SH ZH".split()), set("CH JH".split()),
          set("M N NG".split()), set("L R".split()), set("TH DH".split()), set("W V".split())]


def sub_cost(a, b):
    if a == b:
        return 0.0
    if a in VOWELS and b in VOWELS:
        return 0.35
    if a in VOWELS or b in VOWELS:
        return 1.0
    for g in GROUPS:
        if a in g and b in g:
            return 0.4
    return 1.0


def dist(x, y):
    n, m = len(x), len(y)
    d = [[0.0] * (m + 1) for _ in range(n + 1)]
    for i in range(1, n + 1):
        d[i][0] = i * 0.8
    for j in range(1, m + 1):
        d[0][j] = j * 0.8
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            d[i][j] = min(d[i - 1][j] + 0.8, d[i][j - 1] + 0.8, d[i - 1][j - 1] + sub_cost(x[i - 1], y[j - 1]))
    return d[n][m]


def seq_phones(words):
    out = []
    for w in words:
        out += phones(w)
    return out


# ---- lexicon: closed sets by slot ----
VERBS = ["open", "close", "play", "pause", "search", "type", "stop", "hook up", "shut down", "restart", "show", "list"]
APPS = ["notepad", "brave", "chrome", "spotify", "whatsapp", "discord", "youtube", "vs code", "github", "antigravity", "opencode", "copilot", "cursor"]
OBJECTS_SHUT = ["my laptop", "my computer"]
MODES = ["ghost mode", "hand mode"]
PRIOR = {"notepad": 0.9, "brave": 0.8, "chrome": 0.6, "spotify": 0.7, "whatsapp": 0.8, "antigravity": 0.6, "vs code": 0.7}  # use frequency stand-in


def best(words, lexicon, slot_prior=None, thresh=0.42):
    """Closest lexicon entry to `words` by normalised phoneme distance, with a margin check."""
    hp = seq_phones(words)
    if not hp:
        return None
    scored = []
    for item in lexicon:
        ip = seq_phones(item.split())
        d = dist(hp, ip) / max(len(hp), len(ip))
        d -= 0.04 * (slot_prior or {}).get(item, 0.0)  # tiny nudge from usage prior
        scored.append((d, item))
    scored.sort()
    (d1, a), (d2, _) = scored[0], scored[1]
    if d1 <= thresh and (d2 - d1) >= 0.06:
        return a, d1, d2 - d1
    return None


def fix(utter):
    t = re.sub(r"^(nexus|next is|nex is)[ ,.]+", "", utter.strip().lower()).strip(" .!?")
    w = t.split()
    if len(w) < 2:
        return utter, None
    # sentence shape: <verb> <rest>. Try 1- and 2-word verbs against the closed verb set.
    for vlen in (2, 1):
        if len(w) <= vlen:
            continue
        verb_words, rest = w[:vlen], w[vlen:]
        v = best(verb_words, VERBS, thresh=0.34)
        if not v:
            continue
        verb = v[0]
        if verb in ("open", "close", "play", "pause", "hook up"):
            m = best(rest, APPS, PRIOR)
            if m:
                return f"{verb} {m[0]}", f"verb~{v[1]:.2f} app~{m[1]:.2f} margin {m[2]:.2f}"
        if verb == "shut down":
            m = best(rest, OBJECTS_SHUT)
            if m:
                return f"{verb} {m[0]}", f"verb~{v[1]:.2f}"
    m = best(w, MODES, thresh=0.3)
    if m:
        return m[0], f"mode~{m[1]:.2f}"
    return utter, None


CASES = [  # (heard, intended)  — real log/notes cases first
    ("send down my laptop", "shut down my laptop"), ("send on my laptop", "shut down my laptop"), ("open load pad", "open notepad"),
    ("open note pad", "open notepad"), ("open brief", "open brave"), ("open breath", "open brave"), ("open spot if I", "open spotify"),
    ("open what's up", "open whatsapp"), ("open vs coat", "open vs code"), ("hook up anti pilot", "hook up antigravity"),
    ("hook up anti-gravity", "hook up antigravity"), ("ghost moon", "ghost mode"), ("goes to mode", "ghost mode"),
    ("open this cord", "open discord"), ("open you tube", "open youtube"), ("next is open brief", "open brave"),
    ("opened notepad", "open notepad"), ("open crome", "open chrome"),
]
MUST_NOT_CHANGE = [  # ordinary talk and already-correct commands must pass through untouched
    "give me a brief summary", "open the window", "play some music", "what time is it", "open notepad", "open brave", "close the door please",
    "I think the mission is", "she's a little bit of a ricotta", "open source projects are great", "send an email to prem", "stop",
    "the opening of the movie", "open chrome", "play hold on to it for me",
]

if __name__ == "__main__":
    ok = 0
    print("== fixes (heard -> result | intended)")
    for heard, want in CASES:
        got, why = fix(heard)
        hit = got == want
        ok += hit
        print(f"{'OK  ' if hit else 'MISS'} {heard!r:34} -> {got!r:26} want {want!r} {why or ''}")
    print(f"corrected {ok}/{len(CASES)}")
    bad = 0
    print("== must not change")
    for t in MUST_NOT_CHANGE:
        got, why = fix(t)
        changed = got.strip().lower().strip(" .!?") != t.lower().strip(" .!?")
        bad += changed
        if changed:
            print(f"WRONG {t!r} -> {got!r} {why}")
    print(f"over-corrections {bad}/{len(MUST_NOT_CHANGE)}")
