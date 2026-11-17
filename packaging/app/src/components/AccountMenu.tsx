import { useState } from "react";
import { ipc } from "@/lib/ipc";
import { errorMessage, isQuietError, type OAuthProvider, type Profile } from "@/lib/types";

export function AccountButton({
  profile,
  onSignedOut,
  onSignInClick,
}: {
  profile: Profile | null;
  onSignedOut: () => void;
  onSignInClick: () => void;
}) {
  if (!profile) {
    return (
      <button type="button" onClick={onSignInClick} className="cursor-pointer rounded-md px-2 py-1 text-xs font-medium hover:bg-fog-line">
        Sign in
      </button>
    );
  }
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="max-w-48 truncate text-fog">{profile.email}</span>
      <button
        type="button"
        onClick={() => ipc.signOut().finally(onSignedOut)}
        className="cursor-pointer rounded-md px-2 py-1 font-medium hover:bg-fog-line"
      >
        Sign out
      </button>
    </div>
  );
}

const PROVIDERS: { id: OAuthProvider; label: string; mark: React.ReactNode }[] = [
  {
    id: "google",
    label: "Google",
    mark: <path d="M12 10.2v3.9h5.5c-.2 1.4-1.7 4-5.5 4a6.1 6.1 0 0 1 0-12.2c1.9 0 3.1.8 3.8 1.5l2.6-2.5A9.7 9.7 0 0 0 12 2.3a9.7 9.7 0 1 0 0 19.4c5.6 0 9.3-3.9 9.3-9.5 0-.6-.1-1.100-.2-2H12z" />,
  },
  {
    id: "github",
    label: "GitHub",
    mark: <path d="M12 2a10 10 0 0 0-3.2 19.5c.5.1.7-.2.7-.5v-1.800c-2.800.6-3.400-1.200-3.400-1.200-.5-1.200-1.100-1.500-1.100-1.500-.9-.6.1-.6.1-.6 1 .1 1.500 1 1.500 1 .9 1.500 2.400 1.100 3 .8.1-.7.4-1.100.6-1.400-2.200-.3-4.600-1.100-4.600-5 0-1.100.4-2 1-2.700-.1-.3-.4-1.300.1-2.700 0 0 .8-.3 2.700 1a9.400 9.400 0 0 1 5 0c1.900-1.300 2.700-1 2.700-1 .5 1.400.2 2.400.1 2.700.6.700 1 1.600 1 2.700 0 3.900-2.400 4.700-4.600 5 .4.300.7.900.7 1.800v2.700c0 .3.200.6.700.5A10 10 0 0 0 12 2z" />,
  },
  {
    id: "gitlab",
    label: "GitLab",
    mark: <path d="m12 21.500-4.200-12.900h8.400L12 21.500zM12 21.500 2.300 14.300l1.500-4.600 4 .000L12 21.500zm0 0 9.700-7.200-1.500-4.600h-4L12 21.500zM3.800 9.700 5.700 3.900a.5.500 0 0 1 1 0l1.100 4.700H3.800zm16.400 0-4 0 1.100-4.700a.5.500 0 0 1 1 0l1.900 5.800z" />,
  },
  {
    id: "apple",
    label: "Apple",
    mark: <path d="M16.400 12.700c0-2.400 2-3.500 2.100-3.600-1.200-1.700-3-1.900-3.600-2-1.500-.2-3 .9-3.800.9s-2-.9-3.300-.9c-1.700 0-3.300 1-4.100 2.500-1.800 3.100-.5 7.600 1.300 10.100.8 1.200 1.800 2.600 3.100 2.500 1.200 0 1.700-.8 3.200-.8s1.900.8 3.200.8c1.300 0 2.200-1.200 3-2.400.9-1.400 1.300-2.700 1.300-2.800 0 0-2.400-.9-2.400-3.800zM14 5.600c.7-.8 1.100-1.900 1-3-1 0-2.200.7-2.900 1.500-.6.700-1.200 1.900-1 3 1.100.1 2.200-.6 2.900-1.500z" />,
  },
];

export function SignInForm({ onSignedIn }: { onSignedIn: (p: Profile) => void }) {
  const [mode, setMode] = useState<"signin" | "signup">("signin");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [waiting, setWaiting] = useState<OAuthProvider | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setMessage(null);
    try {
      if (mode === "signup") {
        const text = await ipc.signUp(email, password);
        setMessage({ text, error: false });
        setMode("signin");
      } else {
        onSignedIn(await ipc.signIn(email, password));
      }
    } catch (err) {
      setMessage({ text: errorMessage(err), error: true });
    } finally {
      setBusy(false);
    }
  }

  async function continueWith(provider: OAuthProvider) {
    setWaiting(provider);
    setMessage(null);
    try {
      onSignedIn(await ipc.oauthSignIn(provider));
    } catch (err) {
      if (!isQuietError(err)) setMessage({ text: errorMessage(err), error: true });
    } finally {
      setWaiting(null);
    }
  }

  const field = "w-full rounded-md border border-fog-line bg-transparent px-3 py-2 text-sm outline-none focus:border-ink";

  return (
    <form onSubmit={submit} className="mx-auto mt-16 flex max-w-sm flex-col gap-3">
      <h1 className="text-lg font-semibold">{mode === "signin" ? "Sign in to Onde" : "Create an Onde account"}</h1>
      {waiting ? (
        <div className="flex items-center justify-between rounded-md border border-fog-line px-3 py-2 text-sm">
          <span role="status" className="text-fog">Waiting for the browser…</span>
          <button type="button" onClick={() => void ipc.oauthSignInCancel()} className="cursor-pointer font-medium underline">
            Cancel
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-4 gap-2">
          {PROVIDERS.map((p) => (
            <button
              key={p.id}
              type="button"
              disabled={busy}
              onClick={() => void continueWith(p.id)}
              aria-label={`Continue with ${p.label}`}
              title={`Continue with ${p.label}`}
              className="flex cursor-pointer items-center justify-center rounded-md border border-fog-line py-2.5 hover:bg-fog-line disabled:opacity-50"
            >
              <svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true">{p.mark}</svg>
            </button>
          ))}
        </div>
      )}
      <div className="flex items-center gap-3 text-xs text-fog" aria-hidden="true">
        <span className="h-px flex-1 bg-fog-line" />
        or
        <span className="h-px flex-1 bg-fog-line" />
      </div>
      <input className={field} type="email" placeholder="Email" autoComplete="email" required value={email} onChange={(e) => setEmail(e.target.value)} />
      <input className={field} type="password" placeholder="Password" autoComplete={mode === "signin" ? "current-password" : "new-password"} required value={password} onChange={(e) => setPassword(e.target.value)} />
      <button type="submit" disabled={busy || waiting !== null} className="cursor-pointer rounded-md bg-ink px-3 py-2 text-sm font-medium text-paper disabled:opacity-50">
        {busy ? "Working…" : mode === "signin" ? "Sign in" : "Sign up"}
      </button>
      {message && (
        <p role="status" className={`text-sm ${message.error ? "text-red-600" : "text-fog"}`}>
          {message.text}
        </p>
      )}
      <button type="button" onClick={() => setMode(mode === "signin" ? "signup" : "signin")} className="cursor-pointer text-left text-xs text-fog underline">
        {mode === "signin" ? "No account yet? Sign up" : "Have an account? Sign in"}
      </button>
    </form>
  );
}
