import type { AppSettings } from '$types/models';

export type Theme = 'light' | 'dark' | 'sepia' | 'system';
export type ReaderFont = 'serif' | 'sans' | 'mono';

interface Settings {
	theme: Theme;
	readerFont: ReaderFont;
	readerFontSize: number;
	readerLineHeight: number;
	readerMaxWidth: number;
	readerTextIndent: boolean;
	readerJustify: boolean;
	showEntityHighlights: boolean;
	autoSaveProgress: boolean;
	language: 'zh' | 'en' | 'ja';
	aiProvider: 'deepseek' | 'openai' | 'local';
	embeddingModel: 'qwen3' | 'bge-m3';
}

const STORAGE_KEY = 'nova-reader-settings';

const defaults: Settings = {
	theme: 'system',
	readerFont: 'serif',
	readerFontSize: 18,
	readerLineHeight: 1.8,
	readerMaxWidth: 720,
	readerTextIndent: true,
	readerJustify: true,
	showEntityHighlights: true,
	autoSaveProgress: true,
	language: 'zh',
	aiProvider: 'deepseek',
	embeddingModel: 'qwen3',
};

function loadFromStorage(): Settings {
	if (typeof window === 'undefined') return defaults;
	try {
		const stored = localStorage.getItem(STORAGE_KEY);
		if (stored) return { ...defaults, ...JSON.parse(stored) };
	} catch { /* ignore */ }
	return defaults;
}

class SettingsStore {
	private _settings = $state<Settings>(loadFromStorage());

	get theme() { return this._settings.theme; }
	set theme(v: Theme) { this._settings.theme = v; this.persist(); }

	get readerFont() { return this._settings.readerFont; }
	set readerFont(v: ReaderFont) { this._settings.readerFont = v; this.persist(); }

	get readerFontSize() { return this._settings.readerFontSize; }
	set readerFontSize(v: number) { this._settings.readerFontSize = Math.max(12, Math.min(32, v)); this.persist(); }

	get readerLineHeight() { return this._settings.readerLineHeight; }
	set readerLineHeight(v: number) { this._settings.readerLineHeight = v; this.persist(); }

	get readerMaxWidth() { return this._settings.readerMaxWidth; }
	set readerMaxWidth(v: number) { this._settings.readerMaxWidth = v; this.persist(); }

	get readerTextIndent() { return this._settings.readerTextIndent; }
	set readerTextIndent(v: boolean) { this._settings.readerTextIndent = v; this.persist(); }

	get readerJustify() { return this._settings.readerJustify; }
	set readerJustify(v: boolean) { this._settings.readerJustify = v; this.persist(); }

	get showEntityHighlights() { return this._settings.showEntityHighlights; }
	set showEntityHighlights(v: boolean) { this._settings.showEntityHighlights = v; this.persist(); }

	get autoSaveProgress() { return this._settings.autoSaveProgress; }
	set autoSaveProgress(v: boolean) { this._settings.autoSaveProgress = v; this.persist(); }

	get language() { return this._settings.language; }
	set language(v: 'zh' | 'en' | 'ja') { this._settings.language = v; this.persist(); }

	get aiProvider() { return this._settings.aiProvider; }
	set aiProvider(v: 'deepseek' | 'openai' | 'local') { this._settings.aiProvider = v; this.persist(); }

	get embeddingModel() { return this._settings.embeddingModel; }
	set embeddingModel(v: 'qwen3' | 'bge-m3') { this._settings.embeddingModel = v; this.persist(); }

	get readerStyle(): string {
		const fontFamily = this._settings.readerFont === 'serif'
			? '"Noto Serif SC", "Source Han Serif SC", serif'
			: this._settings.readerFont === 'mono'
				? '"JetBrains Mono", monospace'
				: '"Inter", "Noto Sans SC", sans-serif';
		const indent = this._settings.readerTextIndent ? 'text-indent: 2em;' : '';
		const justify = this._settings.readerJustify ? 'text-align: justify;' : '';
		return `font-family: ${fontFamily}; font-size: ${this._settings.readerFontSize}px; line-height: ${this._settings.readerLineHeight}; max-width: ${this._settings.readerMaxWidth}px; ${indent} ${justify}`;
	}

	private persist() {
		if (typeof window === 'undefined') return;
		localStorage.setItem(STORAGE_KEY, JSON.stringify(this._settings));
	}

	reset() {
		this._settings = { ...defaults };
		this.persist();
	}
}

export const settingsStore = new SettingsStore();

// ─── Feature Flags (server-synced) ──────────────────────────────────────────

export interface FeatureFlags {
	ai_chat: boolean;
	ai_entities: boolean;
	ai_summarize: boolean;
	ai_translate: boolean;
	ai_style_analysis: boolean;
	ai_batch_process: boolean;
	semantic_search: boolean;
	knowledge_graph: boolean;
	reranker: boolean;
}

const defaultFlags: FeatureFlags = {
	ai_chat: true,
	ai_entities: true,
	ai_summarize: true,
	ai_translate: true,
	ai_style_analysis: true,
	ai_batch_process: true,
	semantic_search: true,
	knowledge_graph: true,
	reranker: true,
};

class FeatureFlagStore {
	flags = $state<FeatureFlags>({ ...defaultFlags });
	loaded = $state(false);

	/** Load feature flags from server settings */
	async load(getSettings: () => Promise<AppSettings>) {
		try {
			const settings = await getSettings();
			const features = settings?.ai?.features || settings?.features;
			if (features) {
				this.flags = { ...defaultFlags, ...features };
			}
			this.loaded = true;
		} catch {
			// Use defaults if server unavailable
			this.loaded = true;
		}
	}

	/** Check if a feature is enabled */
	isEnabled(feature: keyof FeatureFlags): boolean {
		return this.flags[feature] ?? true;
	}
}

export const featureFlags = new FeatureFlagStore();
