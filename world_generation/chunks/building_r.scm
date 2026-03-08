(cond
    ((= height 0) (filled-chunk (tile 'concrete)))
    ((= height 1)
        (begin
            (define building-module (default-module 'building))
            (define this-chunk (filled-chunk (tile 'air)))
            (vertical-line this-chunk (- size-x 2) (building-module 'wall-tile))
            (horizontal-line-length this-chunk (make-point 0 (- size-y 1)) (- size-x 1) (building-module 'wall-tile))
            this-chunk))
    (else (filled-chunk (tile 'air))))
