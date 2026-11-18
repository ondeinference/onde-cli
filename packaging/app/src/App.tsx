import { useEffect, useState } from "react";
import { AppTopBar } from "@/components/AppTopBar";
import { AppSidebar } from "@/components/AppSidebar";
import { AccountButton, SignInForm } from "@/components/AccountMenu";
import { NAV_ITEMS, type TabId } from "@/components/nav-items";
import { FinetuneView } from "@/features/finetune/FinetuneView";
import { isRunning } from "@/features/finetune/job";
import { useJob } from "@/features/finetune/useJob";
import { ProjectsView } from "@/features/projects/ProjectsView";
import { isDesktop } from "@/lib/os";
import { ipc } from "@/lib/ipc";
import type { Profile } from "@/lib/types";

const SIDEBAR_KEY = "ondedesktop.sidebarOpen";

export default function App() {
  const [sidebarOpen, setSidebarOpen] = useState(() => localStorage.getItem(SIDEBAR_KEY) !== "false");
  const [tab, setTab] = useState<TabId>(NAV_ITEMS[0].id);
  const [profile, setProfile] = useState<Profile | null>(null);
  const [showSignIn, setShowSignIn] = useState(false);
  const job = useJob();

  useEffect(() => {
    localStorage.setItem(SIDEBAR_KEY, String(sidebarOpen));
  }, [sidebarOpen]);

  useEffect(() => {
    ipc.currentUser().then(setProfile).catch(() => setProfile(null));
  }, []);

  return (
    <>
      <AppTopBar
        sidebarOpen={sidebarOpen}
        onToggleSidebar={() => setSidebarOpen((o) => !o)}
        actions={
          <AccountButton
            profile={profile}
            onSignedOut={() => setProfile(null)}
            onSignInClick={() => setShowSignIn(true)}
          />
        }
      />
      <AppSidebar open={sidebarOpen} active={tab} busyTab={isRunning(job.state) ? "finetune" : undefined} onSelect={(t) => { setTab(t); setShowSignIn(false); }} />
      {/* <main> is the only scroller; the document itself never scrolls (index.css).
          tabIndex makes it focusable so a keyboard user can Tab to it and a click
          lands focus here, keeping space/arrow-key scrolling that overflow:hidden
          takes away from the document. */}
      <main
        tabIndex={0}
        className={isDesktop ? "app-scroll fixed bottom-0 right-0 transition-[left] duration-200" : "app-scroll h-full"}
        style={isDesktop ? {
          top: "var(--titlebar-height)",
          left: sidebarOpen ? "var(--sidebar-width)" : 0,
        } : undefined}
      >
        <div className="mx-auto max-w-2xl px-4 pt-6 pb-6">
          {showSignIn && !profile ? (
            <SignInForm onSignedIn={(p) => { setProfile(p); setShowSignIn(false); }} />
          ) : (
            tab === "projects" ? <ProjectsView /> : <FinetuneView job={job} />
          )}
        </div>
      </main>
    </>
  );
}
