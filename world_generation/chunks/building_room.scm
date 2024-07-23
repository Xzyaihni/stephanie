(define this-chunk (residential-building))

(let ((x (if (= (remainder height 4) 3) 7 8)))
    (put-tile
        this-chunk
        (make-point x 2)
        (tile 'stairs_up)))

(if (= height 1)
    (begin
        ; entrance
        (put-tile
            this-chunk
            (make-point 7 1)
            (tile 'air))

        (put-tile
            this-chunk
            (make-point 8 1)
            (tile 'air)))
    this-chunk)
