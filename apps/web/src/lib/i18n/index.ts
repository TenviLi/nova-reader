// Lightweight i18n system for Nova Reader
// Supports zh-CN and en-US

export type Locale = 'zh-CN' | 'en-US' | 'ja';

const translations: Record<Locale, Record<string, string>> = {
	'zh-CN': {
		// Navigation
		'nav.library': '所有书籍',
		'nav.reading': '阅读列表',
		'nav.search': '搜索',
		'nav.discover': '探索',
		'nav.annotations': '批注',
		'nav.bookmarks': '书签',
		'nav.chat': 'AI 对话',
		'nav.translate': '翻译',
		'nav.entities': '知识图谱',
		'nav.graph': '关系网络',
		'nav.writing': '写作助手',
		'nav.analytics': '统计面板',
		'nav.settings': '系统设置',
		'nav.tasks': '任务队列',
		'nav.users': '用户管理',
		'nav.libraries': '书库管理',

		// Common actions
		'action.save': '保存',
		'action.cancel': '取消',
		'action.delete': '删除',
		'action.edit': '编辑',
		'action.create': '创建',
		'action.search': '搜索',
		'action.filter': '筛选',
		'action.sort': '排序',
		'action.refresh': '刷新',
		'action.upload': '上传',
		'action.download': '下载',
		'action.export': '导出',
		'action.import': '导入',
		'action.retry': '重试',
		'action.close': '关闭',
		'action.confirm': '确认',
		'action.back': '返回',

		// Reading status
		'status.unread': '未读',
		'status.reading': '在读',
		'status.completed': '已读',
		'status.on_hold': '搁置',
		'status.dropped': '弃读',

		// Book processing status
		'book.pending': '待处理',
		'book.processing': '处理中',
		'book.ready': '就绪',
		'book.failed': '失败',

		// Formats
		'format.txt': '纯文本',
		'format.epub': 'EPUB',
		'format.pdf': 'PDF',
		'format.doc': 'Word 97-2003',
		'format.docx': 'Word',
		'format.md': 'Markdown',
		'format.html': '网页',

		// Search
		'search.placeholder': '搜索书名、内容、角色...',
		'search.no_results': '未找到匹配结果',
		'search.results_count': '{count} 条结果',
		'search.mode.keyword': '关键词',
		'search.mode.semantic': '语义',
		'search.mode.hybrid': '混合',
		'search.mode.graph': '图谱',

		// Dashboard
		'dashboard.welcome': '欢迎回来',
		'dashboard.continue_reading': '继续阅读',
		'dashboard.recently_added': '最近添加',
		'dashboard.statistics': '阅读统计',

		// Errors
		'error.network': '网络连接失败',
		'error.unauthorized': '请先登录',
		'error.forbidden': '权限不足',
		'error.not_found': '未找到资源',
		'error.server': '服务器错误',
		'error.upload_too_large': '文件过大（最大 200MB）',

		// AI features
		'ai.companion': '阅读伴侣',
		'ai.spoiler_protection': '防剧透',
		'ai.thinking': '思考中...',
		'ai.sentiment_arc': '情感曲线',
		'ai.analyze': '开始分析',

		// TTS
		'tts.play': '朗读',
		'tts.pause': '暂停',
		'tts.stop': '停止',
		'tts.speed': '语速',
		'tts.voice': '语音',
	},
	'en-US': {
		// Navigation
		'nav.library': 'All Books',
		'nav.reading': 'Reading List',
		'nav.search': 'Search',
		'nav.discover': 'Discover',
		'nav.annotations': 'Annotations',
		'nav.bookmarks': 'Bookmarks',
		'nav.chat': 'AI Chat',
		'nav.translate': 'Translate',
		'nav.entities': 'Knowledge Graph',
		'nav.graph': 'Relations',
		'nav.writing': 'Writing Assistant',
		'nav.analytics': 'Analytics',
		'nav.settings': 'Settings',
		'nav.tasks': 'Task Queue',
		'nav.users': 'User Management',
		'nav.libraries': 'Library Management',

		// Common actions
		'action.save': 'Save',
		'action.cancel': 'Cancel',
		'action.delete': 'Delete',
		'action.edit': 'Edit',
		'action.create': 'Create',
		'action.search': 'Search',
		'action.filter': 'Filter',
		'action.sort': 'Sort',
		'action.refresh': 'Refresh',
		'action.upload': 'Upload',
		'action.download': 'Download',
		'action.export': 'Export',
		'action.import': 'Import',
		'action.retry': 'Retry',
		'action.close': 'Close',
		'action.confirm': 'Confirm',
		'action.back': 'Back',

		// Reading status
		'status.unread': 'Unread',
		'status.reading': 'Reading',
		'status.completed': 'Completed',
		'status.on_hold': 'On Hold',
		'status.dropped': 'Dropped',

		// Book processing status
		'book.pending': 'Pending',
		'book.processing': 'Processing',
		'book.ready': 'Ready',
		'book.failed': 'Failed',

		// Formats
		'format.txt': 'Plain Text',
		'format.epub': 'EPUB',
		'format.pdf': 'PDF',
		'format.doc': 'Word 97-2003',
		'format.docx': 'Word',
		'format.md': 'Markdown',
		'format.html': 'HTML',

		// Search
		'search.placeholder': 'Search titles, content, characters...',
		'search.no_results': 'No results found',
		'search.results_count': '{count} results',
		'search.mode.keyword': 'Keyword',
		'search.mode.semantic': 'Semantic',
		'search.mode.hybrid': 'Hybrid',
		'search.mode.graph': 'Graph',

		// Dashboard
		'dashboard.welcome': 'Welcome back',
		'dashboard.continue_reading': 'Continue Reading',
		'dashboard.recently_added': 'Recently Added',
		'dashboard.statistics': 'Reading Statistics',

		// Errors
		'error.network': 'Network connection failed',
		'error.unauthorized': 'Please sign in',
		'error.forbidden': 'Access denied',
		'error.not_found': 'Resource not found',
		'error.server': 'Server error',
		'error.upload_too_large': 'File too large (max 200MB)',

		// AI features
		'ai.companion': 'Reading Companion',
		'ai.spoiler_protection': 'Spoiler Protection',
		'ai.thinking': 'Thinking...',
		'ai.sentiment_arc': 'Sentiment Arc',
		'ai.analyze': 'Analyze',

		// TTS
		'tts.play': 'Read Aloud',
		'tts.pause': 'Pause',
		'tts.stop': 'Stop',
		'tts.speed': 'Speed',
		'tts.voice': 'Voice',
	},
	'ja': {
		// Navigation
		'nav.library': 'すべての本',
		'nav.reading': '読書リスト',
		'nav.search': '検索',
		'nav.discover': '探索',
		'nav.annotations': '注釈',
		'nav.bookmarks': 'ブックマーク',
		'nav.chat': 'AIチャット',
		'nav.translate': '翻訳',
		'nav.entities': 'ナレッジグラフ',
		'nav.graph': '関係ネットワーク',
		'nav.writing': '執筆アシスタント',
		'nav.analytics': '統計',
		'nav.settings': '設定',
		'nav.tasks': 'タスクキュー',
		'nav.users': 'ユーザー管理',
		'nav.libraries': 'ライブラリ管理',

		// Common actions
		'action.save': '保存',
		'action.cancel': 'キャンセル',
		'action.delete': '削除',
		'action.edit': '編集',
		'action.create': '作成',
		'action.search': '検索',
		'action.filter': 'フィルター',
		'action.sort': '並べ替え',
		'action.refresh': '更新',
		'action.upload': 'アップロード',
		'action.download': 'ダウンロード',
		'action.export': 'エクスポート',
		'action.import': 'インポート',
		'action.retry': '再試行',
		'action.close': '閉じる',
		'action.confirm': '確認',
		'action.back': '戻る',

		// Reading status
		'status.unread': '未読',
		'status.reading': '読書中',
		'status.completed': '読了',
		'status.on_hold': '一時停止',
		'status.dropped': '中断',

		// Search
		'search.placeholder': 'タイトル、内容、キャラクターを検索...',
		'search.no_results': '結果が見つかりません',
		'search.results_count': '{count} 件の結果',

		// Dashboard
		'dashboard.welcome': 'おかえりなさい',
		'dashboard.continue_reading': '続きを読む',
		'dashboard.recently_added': '最近追加',
		'dashboard.statistics': '読書統計',

		// Errors
		'error.network': 'ネットワーク接続失敗',
		'error.unauthorized': 'ログインしてください',
		'error.forbidden': 'アクセス権限がありません',
		'error.not_found': 'リソースが見つかりません',
		'error.server': 'サーバーエラー',

		// AI
		'ai.companion': '読書コンパニオン',
		'ai.thinking': '考え中...',
		'ai.analyze': '分析開始',

		// TTS
		'tts.play': '読み上げ',
		'tts.pause': '一時停止',
		'tts.stop': '停止',
		'tts.speed': '速度',
		'tts.voice': '音声',
	},
};

let currentLocale: Locale = 'zh-CN';

// Reactive locale store for Svelte 5
class LocaleStore {
	current = $state<Locale>('zh-CN');

	setLocale(locale: Locale) {
		this.current = locale;
		currentLocale = locale;
		if (typeof window !== 'undefined') {
			localStorage.setItem('nova_locale', locale);
			document.documentElement.lang = locale;
		}
	}
}

export const localeStore = new LocaleStore();

/**
 * Get the current locale.
 */
export function getLocale(): Locale {
	return currentLocale;
}

/**
 * Set the active locale.
 */
export function setLocale(locale: Locale): void {
	currentLocale = locale;
	localeStore.current = locale;
	if (typeof window !== 'undefined') {
		localStorage.setItem('nova_locale', locale);
		document.documentElement.lang = locale;
	}
}

/**
 * Initialize locale from localStorage or browser preference.
 */
export function initLocale(): void {
	if (typeof window === 'undefined') return;

	const saved = localStorage.getItem('nova_locale') as Locale | null;
	if (saved && translations[saved]) {
		currentLocale = saved;
		localeStore.current = saved;
		return;
	}

	// Auto-detect from browser
	const browserLang = navigator.language;
	if (browserLang.startsWith('zh')) {
		currentLocale = 'zh-CN';
	} else if (browserLang.startsWith('ja')) {
		currentLocale = 'ja';
	} else {
		currentLocale = 'en-US';
	}
	localeStore.current = currentLocale;
}

/**
 * Translate a key, with optional interpolation.
 * Usage: t('search.results_count', { count: 42 }) → "42 条结果"
 */
export function t(key: string, params?: Record<string, string | number>): string {
	const value = translations[currentLocale]?.[key] ?? translations['zh-CN']?.[key] ?? key;

	if (!params) return value;

	return Object.entries(params).reduce(
		(str, [k, v]) => str.replace(`{${k}}`, String(v)),
		value,
	);
}

/**
 * Get all available locales.
 */
export function getAvailableLocales(): { value: Locale; label: string }[] {
	return [
		{ value: 'zh-CN', label: '简体中文' },
		{ value: 'en-US', label: 'English' },
		{ value: 'ja', label: '日本語' },
	];
}
