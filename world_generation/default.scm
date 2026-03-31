(define size-x 8)
(define size-y 8)

(define tile-id car)

(define make-point cons)
(define point-x car)
(define point-y cdr)

(define make-area cons)
(define area-start car)
(define area-size cdr)

(define side-up 0)
(define side-right 1)
(define side-left 2)
(define side-down 3)

(define (single-marker x)
    (cons 'marker (cons x '())))

(define (combine-markers chunk point marker)
    (let ((markers (get-tile chunk point)))
        (if (null? markers)
            (put-tile chunk point (single-marker marker))
            (set-cdr! markers (cons marker (cdr markers))))))

(define (filled-chunk this-tile)
    (make-vector (* size-x size-y) this-tile))

(define (chunk-from-fn f)
    (let ((chunk (filled-chunk (tile 'air))))
        (begin
            (for-each (lambda (x)
                (for-each (lambda (y)
                    (let ((pos (make-point x y)))
                        (put-tile chunk pos (f pos))))
                    (counter size-y)))
                (counter size-y))
            chunk)))

(define (index-of point)
    (+ (* size-x (point-y point)) (point-x point)))

(define (point-add a b) (make-point (+ (point-x a) (point-x b)) (+ (point-y a) (point-y b))))
(define (point-sub a b) (point-add a (make-point (* (point-x b) -1) (* (point-y b) -1))))

(define (point-map a f) (make-point (f (point-x a)) (f (point-y a))))
(define (point-zip-map a b f) (make-point (f (point-x a) (point-x b)) (f (point-y a) (point-y b))))

(define (index-to-pos width index)
    (make-point
        (remainder index width)
        (/ index width)))

(define (area-end area) (point-add (area-start area) (point-sub (area-size area) (make-point 1 1))))
(define (area-area area) (let ((size (area-size area))) (* (point-x size) (point-y size))))

(define (area-index area index)
    (let
        (
            (start (area-start area))
            (width (point-x (area-size area))))
        (make-point
            (+ (remainder index width) (point-x start))
            (+ (/ index width) (point-y start)))))

(define (area-halves-x area)
    (let
        ((this-size (area-size area)))
        (if (< (point-x this-size) 2)
            area
            (let
                ((half-size (make-point (/ (point-x this-size) 2) (/ (point-y this-size) 2))))
                (cons
                    (make-area (area-start area) half-size)
                    (make-area (point-add (area-start area) half-size) (point-sub this-size half-size)))))))

(define (area-point-random area)
    (let
        ((start (area-start area)) (end (area-end area)))
        (make-point
            (random-integer-between (point-x start) (+ (point-x end) 1))
            (random-integer-between (point-y start) (+ (point-y end) 1)))))

(define (side-combine a b)
    (cond
        ((= a side-up) b)
        ((= a side-right) (cond ((= b side-up) side-right) ((= b side-right) side-down) ((= b side-down) side-left) ((= b side-left) side-up)))
        ((= a side-down) (cond ((= b side-up) side-down) ((= b side-right) side-left) ((= b side-down) side-up) ((= b side-left) side-right)))
        (else (cond ((= b side-up) side-left) ((= b side-right) side-up) ((= b side-down) side-right) ((= b side-left) side-down)))))

(define (side-horizontal? x) (or (= x side-left) (= x side-right)))
(define (side-vertical? x) (not (side-horizontal? x)))

(define (side-flip-x x)
    (cond
        ((= x side-right) side-left)
        ((= x side-left) side-right)
        (else x)))

(define (side-opposite x)
    (cond
        ((= x side-up) side-down)
        ((= x side-right) side-left)
        ((= x side-left) side-right)
        (else side-up)))

(define (side-name x)
    (cond
        ((= x side-up) "up")
        ((= x side-right) "right")
        ((= x side-left) "left")
        (else "down")))

(define (position-at-side position side)
    (let
        ((absolute-side (side-combine rotation side)))
        (let
            ((offset (cond
                ((= absolute-side side-up) (cons 0 -1))
                ((= absolute-side side-down) (cons 0 1))
                ((= absolute-side side-left) (cons -1 0))
                ((= absolute-side side-right) (cons 1 0)))))
            (let
                ((i (car position)) (tail (cdr position)))
                (let
                    ((x (car tail)) (tail (cdr tail)))
                    (cons
                        i
                        (cons
                            (+ (car offset) x)
                            (cons (+ (cdr offset) (car tail)) (cdr tail)))))))))

(define (put-tile chunk pos this-tile)
    (vector-set!
        chunk
        (index-of pos)
        this-tile)
    chunk)

(define (get-tile chunk pos)
    (vector-ref chunk (index-of pos)))

(define (for-each-tile f area)
    (define (for-vertical pos len)
        (if (not (= len 0))
            (begin
                (f (make-point (point-x pos) (- (+ len (point-y pos)) 1)))
                (for-vertical pos (- len 1)))))
    (define pos (area-start area))
    (define size (area-size area))
    (if (not (= (point-x size) 0))
        (begin
            (for-vertical
                (make-point (- (+ (point-x pos) (point-x size)) 1) (point-y pos))
                (point-y size))
            (for-each-tile
                f
                (make-area pos (make-point (- (point-x size) 1) (point-y size)))))))

(define (vertical-line-length chunk pos len this-tile)
    (for-each-tile
        (lambda (pos) (put-tile chunk pos this-tile))
        (make-area
            pos
            (make-point 1 len)))
    chunk)

(define (vertical-line chunk x this-tile)
    (vertical-line-length chunk (make-point x 0) size-y this-tile))

(define (horizontal-line-length chunk pos len this-tile)
    (for-each-tile
        (lambda (pos) (put-tile chunk pos this-tile))
        (make-area
            pos
            (make-point len 1)))
    chunk)

(define (horizontal-line chunk y this-tile)
    (horizontal-line-length chunk (make-point 0 y) size-x this-tile))

(define (fill-area chunk area this-tile)
    (for-each-tile
        (lambda (pos) (put-tile chunk pos this-tile))
        area)
    chunk)

(define (copy-area chunk area offset)
    (for-each-tile
        (lambda (pos) (put-tile chunk (point-add pos offset) (get-tile chunk pos)))
        area)
    chunk)

; if the destination overlaps the area it will get cut off
(define (move-area chunk area offset)
    (copy-area chunk area offset)
    (fill-area
        chunk
        area
        (tile 'air)))

(define (rectangle-outline-different chunk area up right left down)
    (define pos (area-start area))
    (define size (area-size area))
    (vertical-line-length
        chunk
        pos
        (point-y size)
        left)

    (vertical-line-length
        chunk
        (make-point (- (+ (point-x pos) (point-x size)) 1) (point-y pos))
        (point-y size)
        right)

    (horizontal-line-length
        chunk
        pos
        (point-x size)
        up)

    (horizontal-line-length
        chunk
        (make-point (point-x pos) (- (+ (point-y pos) (point-y size)) 1))
        (point-x size)
        down))

(define (rectangle-outline chunk area this-tile)
    (rectangle-outline-different chunk area this-tile this-tile this-tile this-tile))

(define (pick-weighted a b value)
    (if (< (random-float) value)
        b
        a))

(define (gradient-pick xs value start end)
    (let ((total (length xs)))
        (let ((index-fractional (* (/ (- total 1) end) value)))
            (let ((start-index (inexact->exact (floor index-fractional))))
                (if (< start-index (- total 1))
                    (pick-weighted
                        (list-ref xs start-index)
                        (list-ref xs (+ start-index 1))
                        (remainder index-fractional 1))
                    (list-ref xs (- total 1)))))))

(define (seed-with seed number)
    (random-integer-seeded (wrapping-add seed number)))

(define (difficulty-chance scale start) (< (random-float) (+ (* difficulty scale) start)))

(define (stop-between-difficulty start end)
    (if (< difficulty start)
        #t
        (if (> difficulty end)
            #f
            (let ((fraction (/ (- difficulty start) (- end start))))
                (> (random-float) fraction)))))
