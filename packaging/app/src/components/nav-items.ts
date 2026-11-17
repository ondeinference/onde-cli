// Only features that exist get a tab. Library, Chat, Fine-tune and Apps come
// back in the change that builds each one.
export const NAV_ITEMS = [
  { id: "projects", label: "Projects" },
] as const;

export type TabId = (typeof NAV_ITEMS)[number]["id"];
