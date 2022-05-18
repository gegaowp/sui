import { createAsyncThunk } from '@reduxjs/toolkit';

import type { ThunkExtras } from './thunk-extras';
import type { AsyncThunk, AsyncThunkPayloadCreator } from '@reduxjs/toolkit';

interface AppThunkConfig {
    extra: ThunkExtras;
}

export function createAppAsyncThunk<
    Returned,
    ThunkArg = void,
    ThunkConfig extends AppThunkConfig = AppThunkConfig
>(
    typePrefix: string,
    payloadCreator: AsyncThunkPayloadCreator<Returned, ThunkArg, ThunkConfig>
): AsyncThunk<Returned, ThunkArg, ThunkConfig> {
    return createAsyncThunk<Returned, ThunkArg, ThunkConfig>(
        typePrefix,
        payloadCreator
    );
}
