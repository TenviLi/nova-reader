import type { Book } from '$types/models';

export interface ChatMessage {
	id: string;
	role: 'user' | 'assistant' | 'system';
	content: string;
	timestamp: number;
	metadata?: {
		sources?: Array<{ book_id: string; book_title: string; chapter: string; excerpt: string }>;
		model?: string;
		tokens_used?: number;
	};
}

export interface ChatSession {
	id: string;
	title: string;
	messages: ChatMessage[];
	context: {
		book_id?: string;
		include_rag: boolean;
		system_prompt?: string;
	};
	created_at: number;
	updated_at: number;
}

const SESSIONS_KEY = 'nova-chat-sessions';

class ChatService {
	sessions = $state<ChatSession[]>([]);
	activeSessionId = $state<string | null>(null);

	activeSession = $derived(
		this.sessions.find(s => s.id === this.activeSessionId) ?? null
	);

	constructor() {
		if (typeof window !== 'undefined') {
			this.loadFromStorage();
		}
	}

	private loadFromStorage() {
		try {
			const stored = localStorage.getItem(SESSIONS_KEY);
			if (stored) this.sessions = JSON.parse(stored);
		} catch { /* ignore */ }
	}

	private persist() {
		if (typeof window === 'undefined') return;
		localStorage.setItem(SESSIONS_KEY, JSON.stringify(this.sessions));
	}

	createSession(options?: { bookId?: string; title?: string; systemPrompt?: string }): string {
		const id = crypto.randomUUID();
		const session: ChatSession = {
			id,
			title: options?.title ?? '新对话',
			messages: [],
			context: {
				book_id: options?.bookId,
				include_rag: true,
				system_prompt: options?.systemPrompt,
			},
			created_at: Date.now(),
			updated_at: Date.now(),
		};
		this.sessions = [session, ...this.sessions];
		this.activeSessionId = id;
		this.persist();
		return id;
	}

	deleteSession(id: string) {
		this.sessions = this.sessions.filter(s => s.id !== id);
		if (this.activeSessionId === id) {
			this.activeSessionId = this.sessions[0]?.id ?? null;
		}
		this.persist();
	}

	addMessage(sessionId: string, message: Omit<ChatMessage, 'id' | 'timestamp'>) {
		const session = this.sessions.find(s => s.id === sessionId);
		if (!session) return;

		const fullMessage: ChatMessage = {
			...message,
			id: crypto.randomUUID(),
			timestamp: Date.now(),
		};

		session.messages = [...session.messages, fullMessage];
		session.updated_at = Date.now();

		// Auto-title from first user message
		if (session.messages.length === 1 && message.role === 'user') {
			session.title = message.content.slice(0, 40) + (message.content.length > 40 ? '...' : '');
		}

		this.persist();
		return fullMessage;
	}

	async *streamResponse(sessionId: string): AsyncGenerator<string, void, unknown> {
		const session = this.sessions.find(s => s.id === sessionId);
		if (!session) return;

		const messages = session.messages.map(m => ({
			role: m.role,
			content: m.content,
		}));

		try {
			const response = await fetch('/api/ai/chat/stream', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					messages,
					book_id: session.context.book_id,
					include_rag: session.context.include_rag,
					system_prompt: session.context.system_prompt,
				}),
			});

			if (!response.ok) throw new Error(`HTTP ${response.status}`);
			if (!response.body) throw new Error('No response body');

			const reader = response.body.getReader();
			const decoder = new TextDecoder();
			let buffer = '';

			while (true) {
				const { done, value } = await reader.read();
				if (done) break;

				buffer += decoder.decode(value, { stream: true });
				const lines = buffer.split('\n');
				buffer = lines.pop() ?? '';

				for (const line of lines) {
					if (line.startsWith('data: ')) {
						const data = line.slice(6);
						if (data === '[DONE]') return;
						try {
							const parsed = JSON.parse(data);
							if (parsed.content) yield parsed.content;
						} catch { /* skip malformed */ }
					}
				}
			}
		} catch (error) {
			yield `\n\n⚠️ 连接失败: ${error instanceof Error ? error.message : '未知错误'}`;
		}
	}
}

export const chatService = new ChatService();
