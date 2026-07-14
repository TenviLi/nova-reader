type ShortcutHandler = () => void;

interface Shortcut {
	key: string;
	meta?: boolean;
	ctrl?: boolean;
	shift?: boolean;
	alt?: boolean;
	handler: ShortcutHandler;
	description: string;
}

class KeyboardShortcuts {
	private shortcuts: Shortcut[] = [];
	private enabled = true;

	register(shortcut: Shortcut) {
		this.shortcuts.push(shortcut);
		return () => this.unregister(shortcut);
	}

	unregister(shortcut: Shortcut) {
		this.shortcuts = this.shortcuts.filter(s => s !== shortcut);
	}

	disable() {
		this.enabled = false;
	}

	enable() {
		this.enabled = true;
	}

	handleKeydown(e: KeyboardEvent) {
		if (!this.enabled) return;

		// Skip when typing in input elements
		const target = e.target as HTMLElement;
		if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
			// Allow ⌘K to always work
			if (!(e.metaKey && e.key === 'k')) return;
		}

		for (const shortcut of this.shortcuts) {
			const keyMatch = e.key.toLowerCase() === shortcut.key.toLowerCase();
			const metaMatch = !!shortcut.meta === e.metaKey;
			const ctrlMatch = !!shortcut.ctrl === e.ctrlKey;
			const shiftMatch = !!shortcut.shift === e.shiftKey;
			const altMatch = !!shortcut.alt === e.altKey;

			if (keyMatch && metaMatch && ctrlMatch && shiftMatch && altMatch) {
				e.preventDefault();
				shortcut.handler();
				return;
			}
		}
	}

	getAll(): Array<{ keys: string; description: string }> {
		return this.shortcuts.map(s => {
			const parts: string[] = [];
			if (s.meta) parts.push('⌘');
			if (s.ctrl) parts.push('⌃');
			if (s.alt) parts.push('⌥');
			if (s.shift) parts.push('⇧');
			parts.push(s.key.toUpperCase());
			return { keys: parts.join(''), description: s.description };
		});
	}
}

export const shortcuts = new KeyboardShortcuts();

// Reader-specific shortcuts factory
export function createReaderShortcuts(actions: {
	nextChapter: () => void;
	prevChapter: () => void;
	toggleSidebar: () => void;
	toggleFullscreen: () => void;
	increaseFontSize: () => void;
	decreaseFontSize: () => void;
	bookmark: () => void;
	search: () => void;
}) {
	const scrollPage = (direction: 1 | -1) => {
		const container = document.getElementById('reader-scroll-container') || document.documentElement;
		const pageHeight = container.clientHeight * 0.85;
		container.scrollBy({ top: direction * pageHeight, behavior: 'smooth' });
	};

	const scrollLine = (direction: 1 | -1) => {
		const container = document.getElementById('reader-scroll-container') || document.documentElement;
		container.scrollBy({ top: direction * 80, behavior: 'smooth' });
	};

	const unsubscribers = [
		shortcuts.register({ key: 'ArrowRight', handler: actions.nextChapter, description: '下一章' }),
		shortcuts.register({ key: 'ArrowLeft', handler: actions.prevChapter, description: '上一章' }),
		shortcuts.register({ key: 'n', handler: actions.nextChapter, description: '下一章' }),
		shortcuts.register({ key: 'p', handler: actions.prevChapter, description: '上一章' }),
		shortcuts.register({ key: 'j', handler: () => scrollLine(1), description: '向下滚动' }),
		shortcuts.register({ key: 'k', handler: () => scrollLine(-1), description: '向上滚动' }),
		shortcuts.register({ key: ' ', handler: () => scrollPage(1), description: '下一页' }),
		shortcuts.register({ key: 'PageDown', handler: () => scrollPage(1), description: '下一页' }),
		shortcuts.register({ key: 'PageUp', handler: () => scrollPage(-1), description: '上一页' }),
		shortcuts.register({ key: 's', meta: true, handler: actions.toggleSidebar, description: '切换侧栏' }),
		shortcuts.register({ key: 'f', meta: true, shift: true, handler: actions.toggleFullscreen, description: '全屏' }),
		shortcuts.register({ key: '=', meta: true, handler: actions.increaseFontSize, description: '增大字号' }),
		shortcuts.register({ key: '-', meta: true, handler: actions.decreaseFontSize, description: '减小字号' }),
		shortcuts.register({ key: 'b', meta: true, handler: actions.bookmark, description: '添加书签' }),
		shortcuts.register({ key: 'f', meta: true, handler: actions.search, description: '搜索' }),
	];

	return () => unsubscribers.forEach(unsub => unsub());
}
