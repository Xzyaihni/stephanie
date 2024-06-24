(define (put-entrance side chunk)
    (fill-area chunk (make-point 7 0) (make-point 1 1) (tile (quote air))))

(let ((wall-material (tile (quote concrete)))
        (entrance-side random-side))
    (put-entrance
        entrance-side
        (horizontal-line
            (horizontal-line
                (vertical-line
                    (vertical-line
                        (filled-chunk (tile (quote air)))
                        0
                        wall-material)
                    (- size-x 1)
                    wall-material)
                0
                wall-material)
            (- size-y 1)
            wall-material)))
