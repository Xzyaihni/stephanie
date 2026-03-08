(cond
    ((= height 0) (filled-chunk (tile 'concrete)))
    ((= height 1)
        (begin
            (define building-module (default-module 'building))
            (define this-chunk (filled-chunk (tile 'air)))
            (horizontal-line-length this-chunk (make-point 0 (- size-y 2)) (- size-x 1) (building-module 'wall-tile))
            (vertical-line-length this-chunk (make-point (- size-x 2) 0) (- size-y 1) (building-module 'wall-tile))
            this-chunk))
    (else (filled-chunk (tile 'air))))
