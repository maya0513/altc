# array・slice・range-for

固定長arrayからmutable sliceを作り、既知範囲のindex、`.get`によるchecked pointer取得、const viewへの明示変換、range-forを同じ処理で使う。評価順を隠すiteratorやallocationを生成せず、終了code 20を返す。
