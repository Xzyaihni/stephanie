(define (line-tile flip) (tile 'asphalt-line (side-combine rotation (if flip side-right side-up))))

(define horizontal-line (line-tile #f))
(define vertical-line (line-tile #t))

(define this-chunk
    (fill-area
        (fill-area
            (fill-area
                (fill-area
                    (filled-chunk (tile 'concrete))
                    (make-area
                        (make-point 0 2)
                        (make-point (- size-x 2) (- size-y 4)))
                    (tile 'asphalt))
                (make-area
                    (make-point 2 0)
                    (make-point (- size-x 4) 2))
                (tile 'asphalt))
            (make-area
                (make-point 0 (- (/ size-y 2) 1))
                (make-point (- size-x 9) 2))
            horizontal-line)
        (make-area
            (make-point (- (/ size-x 2) 1) 0)
            (make-point 2 (- size-y 9)))
        vertical-line))

(define (this-put-tile pos t) (put-tile this-chunk pos t))

(this-put-tile (make-point 7 8) horizontal-line)
(this-put-tile (make-point 8 7) vertical-line)

(define corner (tile 'asphalt-line-l (side-combine rotation side-left)))

(this-put-tile (make-point 8 8) corner)
(this-put-tile (make-point 7 7) corner)
