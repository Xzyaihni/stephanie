(define (put-entrance side chunk)
    (put-tile chunk (make-point 7 1) (tile 'air)))

(let ((wall-material (tile 'concrete))
        (entrance-side random-side))
    (put-entrance
        entrance-side
        (horizontal-line-length
            (horizontal-line-length
                (vertical-line-length
                    (vertical-line-length
                        (filled-chunk (tile 'air))
                        (make-point 1 1)
                        (- size-y 2)
                        wall-material)
                    (make-point (- size-x 2) 1)
                    (- size-y 2)
                    wall-material)
                (make-point 1 1)
                (- size-x 2)
                wall-material)
            (make-point 1 (- size-y 2))
            (- size-x 2)
            wall-material)))
