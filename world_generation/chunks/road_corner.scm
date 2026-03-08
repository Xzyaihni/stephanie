(define (line-tile flip) (tile 'asphalt-line (side-combine rotation (if flip side-right side-up))))

(define path-tile (tile 'concrete-path))

(define horizontal-line-tile (line-tile #f))
(define vertical-line-tile (line-tile #t))

(define this-chunk
    (fill-area
        (fill-area
            (horizontal-line
                (vertical-line
                    (put-tile
                        (filled-chunk (tile 'asphalt))
                        (make-point 0 0)
                        path-tile)
                    (- size-x 1)
                    path-tile)
                (- size-y 1)
                path-tile)
            (make-area
                (make-point 0 (- (/ size-y 2) 1))
                (make-point 3 2))
            horizontal-line-tile)
        (make-area
            (make-point (- (/ size-x 2) 1) 0)
            (make-point 2 3))
        vertical-line-tile))

(define (this-put-tile pos t) (put-tile this-chunk pos t))

(this-put-tile (make-point 3 4) horizontal-line-tile)
(this-put-tile (make-point 4 3) vertical-line-tile)

(define corner (tile 'asphalt-line-l (side-combine rotation side-left)))

(this-put-tile (make-point 3 3) corner)
(this-put-tile (make-point 4 4) corner)
