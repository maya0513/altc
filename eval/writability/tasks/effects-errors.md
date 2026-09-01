# effect・typed error

負値をtyped errorとしてraiseする関数と、それを`?`で伝播する関数を作る。後者はdeferで
C関数を呼び、entry pointはtry/catchでerrorを値へ戻す。成功時にもcleanupを実行する。
