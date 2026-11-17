export const NAV_ITEMS = [
  { id: "library", label: "Library", blurb: "Local models, Hugging Face search and downloads." },
  { id: "chat", label: "Chat", blurb: "Talk to a local GGUF model." },
  { id: "finetune", label: "Fine-tune", blurb: "Train a LoRA adapter, merge it and export a GGUF." },
  { id: "projects", label: "Projects", blurb: "Datasets and runs for each fine-tune." },
  { id: "apps", label: "Apps", blurb: "Publish a model and assign it to an Onde app." },
] as const;

export type TabId = (typeof NAV_ITEMS)[number]["id"];
