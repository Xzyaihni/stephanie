(cond
    ((= height 0) (filled-chunk (tile 'concrete)))
    ((= height 1)
        (begin
            (define building-module (default-module 'building))
            (define this-chunk (filled-chunk (tile 'air)))
            (horizontal-line this-chunk 0 (building-module 'wall-tile))
            (vertical-line this-chunk 0 (building-module 'wall-tile))
            (vertical-line this-chunk (- size-x 1) (building-module 'wall-tile))
            (horizontal-line-length this-chunk (make-point 3 0) 2 (tile 'air))
            (put-tile
                this-chunk
                (make-point 3 0)
                (single-marker (list 'door side-left 'metal 2)))
            this-chunk))
    (else (filled-chunk (tile 'air))))
