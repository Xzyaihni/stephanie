(if (= height 0)
    (fill-area
        (fill-area
            (filled-chunk (tile 'concrete))
            (make-area
                (make-point 0 2)
                (make-point size-x (- size-y 4)))
            (tile 'asphalt))
        (make-area
            (make-point 0 (- (/ size-y 2) 1))
            (make-point size-x 2))
        (tile (if (side-horizontal? rotation) 'asphalt-line-vertical 'asphalt-line-horizontal)))
    (begin

    (define this-chunk (filled-chunk (tile 'air)))

    (define (decide-enemy type)
        (if (eq? type 'normal)
            (pick-weighted 'zob 'runner 0.25)
            'bigy))

    (define (place-enemy point)
        (combine-markers
            this-chunk
            point
            (list
                'enemy
                (decide-enemy
                    (gradient-pick
                        '(normal strong)
                        difficulty
                        0.1
                        3.0)))))

    (define (maybe-enemy point)
        (if (difficulty-chance 0.5 0.1)
            (place-enemy point)))

    (for-each
        (lambda (y) (maybe-enemy (make-point (random-integer-between 0 size-x) y)))
        (range 2 (- size-y 2)))

    this-chunk))
