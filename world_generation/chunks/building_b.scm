(cond
    ((= height 0) (filled-chunk (tile 'concrete)))
    ((= height 1)
        (begin
            (define building-module (default-module 'building))
            (define this-chunk (filled-chunk (tile 'air)))
            (horizontal-line this-chunk (- size-y 2) (building-module 'wall-tile))
            this-chunk))
    (else (filled-chunk (tile 'air))))
