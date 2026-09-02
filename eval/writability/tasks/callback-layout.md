# extern C object・layout・callback

extern Cのglobal object、typedef struct、tagged enum、callback引数を宣言する。生成FFI TUがconstness、size/alignment/offset/field型、enumerator、callback signatureをheaderに照合し、altc functionをC callbackとして実行できることを求める。
