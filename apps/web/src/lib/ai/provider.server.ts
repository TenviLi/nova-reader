/**
 * DeepSeek provider configuration (server-only).
 * Uses @ai-sdk/openai with custom baseURL pointing to DeepSeek API.
 */
import { createOpenAI } from '@ai-sdk/openai';
import { env } from '$env/dynamic/private';

export function getProvider() {
	const baseURL = env.DEEPSEEK_BASE_URL || 'https://api.deepseek.com/v1';
	const apiKey = env.DEEPSEEK_API_KEY || '';

	return createOpenAI({
		baseURL: baseURL.endsWith('/v1') ? baseURL : `${baseURL}/v1`,
		apiKey,
		name: 'deepseek',
	});
}

export function getModel() {
	return env.DEEPSEEK_MODEL || 'deepseek-chat';
}

/** System prompt for the reading assistant */
export const SYSTEM_PROMPT = `你是 Nova Reader 的 AI 阅读助手。你的职责是帮助用户理解和分析他们书库中的书籍内容。

规则：
- 基于提供的上下文回答问题，如果信息不足请坦诚说明
- 使用中文回答，除非用户使用其他语言提问
- 回答简洁有据，适当引用原文
- 对于文学分析，提供多角度解读
- 支持 Markdown 格式输出`;
