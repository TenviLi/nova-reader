/**
 * Reusable async action wrapper with loading state and toast feedback.
 * PLT: Monad-like composition — wraps effectful operations with
 * standardized error handling and UI feedback.
 *
 * Usage:
 *   const save = useAction(async () => { await api.updateBook(id, data) }, {
 *     success: '保存成功',
 *     error: '保存失败',
 *   });
 *   <button disabled={save.loading} onclick={save.run}>保存</button>
 */
import { toast } from 'svelte-sonner';
import { getErrorMessage } from '$lib/utils';

interface ActionOptions {
	success?: string;
	error?: string;
	/** Called on success with the result */
	onSuccess?: (result: unknown) => void;
}

export function useAction<T>(fn: () => Promise<T>, options: ActionOptions = {}) {
	let loading = $state(false);

	async function run(): Promise<T | undefined> {
		loading = true;
		try {
			const result = await fn();
			if (options.success) toast.success(options.success);
			options.onSuccess?.(result);
			return result;
		} catch (e: unknown) {
			const msg = getErrorMessage(e);
			toast.error(options.error ? `${options.error}: ${msg}` : msg);
			return undefined;
		} finally {
			loading = false;
		}
	}

	return {
		get loading() { return loading; },
		run,
	};
}
