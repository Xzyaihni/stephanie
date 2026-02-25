(if (= height 0)
    (begin
        (define this-chunk
            (fill-area
                (fill-area
                    (filled-chunk (tile 'concrete-path))
                    (make-area
                        (make-point 0 2)
                        (make-point size-x (- size-y 4)))
                    (tile 'asphalt))
                (make-area
                    (make-point 0 (- (/ size-y 2) 1))
                    (make-point size-x 2))
                (tile 'asphalt-line rotation)))
        (let
            (
                (try-path
                    (lambda (side line-y)
                        (let
                            ((check-chunk (chunk-at (position-at-side position side))))
                            (if (and (eq? (car check-chunk) 'building) (= (cdr check-chunk) (side-combine rotation (side-opposite side))))
                                (let
                                    (
                                        (r (random-integer-between 0 3))
                                        (make-path-with
                                            (lambda (path-tile)
                                                (horizontal-line-length this-chunk (make-point 1 line-y) (- size-x 3) path-tile))))
                                    (cond
                                        ((= r 0) (make-path-with (tile 'concrete-path-tiled)))
                                        ((= r 1) (make-path-with (tile 'brick-path))))))))))
            (begin (try-path side-up 0) (try-path side-down (- size-y 1))))
        this-chunk)
    (begin
        (define this-chunk (filled-chunk (tile 'air)))

        (define guaranteed-height (random-integer-between 0 size-y))

        (define (decide-enemy type)
            (cond
                ((eq? type 'easy) (pick-weighted 'old 'smol 0.25))
                ((eq? type 'normal) (pick-weighted 'zob 'runner 0.25))
                (else 'bigy)))

        (define (place-enemy point)
            (combine-markers
                this-chunk
                point
                (list
                    'enemy
                    (decide-enemy
                        (gradient-pick
                            '(easy normal strong)
                            difficulty
                            0.1
                            3.0)))))

        (define (maybe-enemy point)
            (if (difficulty-chance 0.5 0.1)
                (place-enemy point)))

        (for-each
            (lambda (y)
                (let ((point (make-point (random-integer-between 0 size-x) y)))
                    (if (= y guaranteed-height)
                        (place-enemy point)
                        (maybe-enemy point))))
            (range 2 (- size-y 2)))

        this-chunk))
