import { streamText, type UIMessage, convertToModelMessages } from 'ai';
import { getProvider, getModel, SYSTEM_PROMPT } from '$lib/ai/provider.server';
import type { RequestHandler } from './$types';

const BACKEND_URL = 'http://localhost:3000';

export const POST: RequestHandler = async ({ request, cookies }) => {
	const { messages, bookId, includeRag } = (await request.json()) as {
		messages: UIMessage[];
		bookId?: string;
		includeRag?: boolean;
	};

	// Build system prompt with optional RAG context
	let systemPrompt = SYSTEM_PROMPT;

	if (includeRag && bookId) {
		const ragContext = await fetchRagContext(bookId, messages, cookies.get('nova_token'));
		if (ragContext) {
			systemPrompt += `\n\n<context type="retrieved_knowledge">
以下信息来自用户的个人书库，是与当前对话相关的检索结果。请基于这些信息回答问题，如果信息不足以回答，请坦诚说明。

${ragContext}
</context>`;
		}
	}

	const provider = getProvider();
	const model = getModel();

	const result = streamText({
		model: provider(model),
		system: systemPrompt,
		messages: await convertToModelMessages(messages),
	});

	return result.toUIMessageStreamResponse();
};

async function fetchRagContext(
	bookId: string,
	messages: UIMessage[],
	token?: string
): Promise<string | null> {
	// Extract latest user query from messages
	const lastUserMsg = [...messages].reverse().find((m) => m.role === 'user');
	const query =
		lastUserMsg?.parts?.find((p) => p.type === 'text')?.text || '';

	if (!query) return null;

	try {
		const headers: Record<string, string> = { 'Content-Type': 'application/json' };
		if (token) {
			headers['Cookie'] = `nova_token=${token}`;
		}

		const res = await fetch(`${BACKEND_URL}/api/ai/rag-context`, {
			method: 'POST',
			headers,
			body: JSON.stringify({ query, book_id: bookId }),
		});

		if (!res.ok) return null;

		const data = await res.json();
		// Unwrap envelope format
		const payload = data?.data ?? data;
		return payload?.context || null;
	} catch {
		return null;
	}
}
